//! 降智检测后台任务：按节奏给正常账号出题、复测并在确认降智后停号
//!
//! 周期只负责排期和发起，每轮考试在独立的后台任务里执行，周期不等待考试结束

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use futures::StreamExt as _;
use gateway_core::account::ProviderAccountId;
use gateway_core::engine::probe::AccountProbeTarget;
use gateway_core::routing::UpstreamModelId;
use gateway_core::task::{
    ScheduledTask, WorkerContribution, WorkerCycleContext, WorkerId, WorkerKind,
    WorkerLeaseRequest, WorkerRegistration, WorkerRunnable, WorkerSchedule, WorkerTaskError,
};
use tracing::warn;

use super::judge::{judge, round_outcome};
use crate::freeze_recovery::{
    WORKER_INITIAL_BACKOFF, WORKER_LEASE_RENEWAL, WORKER_LEASE_TTL, WORKER_MAXIMUM_BACKOFF,
};
use crate::model::accounts::{
    AccountConnectionTestEvent, AccountConnectionTestOptions, AccountListQuery, AccountStatus,
    BatchUpdateAccounts,
};
use crate::model::iq_probe::{
    IQ_PROBE_ANSWER_MAX_CHARS, IQ_PROBE_BREAKER_WINDOW_MINUTES, IqProbeAction, IqProbeAttempt,
    IqProbeAttemptVerdict, IqProbeKind, IqProbeOutcome, IqProbeQuestion, IqProbeQuestionResult,
    IqProbeQuestionVerdict, IqProbeRound, IqProbeRoundDetail, IqProbeScheduleRow, IqProbeSettings,
};
use crate::model::{AdminError, MutationActor, MutationContext, PageSize};
use crate::ports::iq_probe::IqProbeStore;
use crate::ports::store::{AccountRuntimeStore, AccountStore};
use crate::use_case::accounts::AccountsService;

/// 周期只负责排期和发起考试，考试本身在后台执行，不受周期长短限制
pub const IQ_PROBE_INTERVAL: Duration = Duration::from_secs(15);
/// 复用冻结恢复的 Worker 类型，以独立 owner 区分，避免改动 Core 的任务枚举
pub const IQ_PROBE_WORKER_OWNER: &str = "account-iq-probe";

/// 单次提问的最长等待；高推理强度的回答可能较慢
const ATTEMPT_TIMEOUT: Duration = Duration::from_secs(180);
/// 轮次结果只保留 7 天
const ROUND_RETENTION_DAYS: i64 = 7;
/// 清理过期轮次的最短间隔
const ROUND_PURGE_INTERVAL_MINUTES: i64 = 60;
/// 停号日志里回答的最大字符数
const LOG_ANSWER_CHARS: usize = 200;
/// 失败原因摘要的最大字符数
const ERROR_SUMMARY_CHARS: usize = 300;

/// 降智检测所需的管理端口组合
#[derive(Clone)]
pub struct IqProbeWorkerDeps {
    pub accounts: Arc<dyn AccountsService>,
    pub store: Arc<dyn AccountStore>,
    pub runtime: Arc<dyn AccountRuntimeStore>,
    pub probes: Arc<dyn IqProbeStore>,
}

pub struct IqProbeTask {
    runner: RoundRunner,
    /// 正在考试的账号；同一账号上一轮没结束时不开新一轮
    running: Arc<Mutex<HashSet<String>>>,
    last_purge_at: Mutex<Option<DateTime<Utc>>>,
}

impl IqProbeTask {
    pub fn new(deps: IqProbeWorkerDeps) -> Self {
        Self {
            runner: RoundRunner {
                deps,
                disable_lock: Arc::new(tokio::sync::Mutex::new(())),
            },
            running: Arc::new(Mutex::new(HashSet::new())),
            last_purge_at: Mutex::new(None),
        }
    }

    pub(crate) fn into_worker(self) -> Result<WorkerContribution, AdminError> {
        let id = WorkerId::try_new(WorkerKind::AccountFreezeRecovery, IQ_PROBE_WORKER_OWNER)
            .map_err(|_| AdminError::internal("降智检测 Worker ID 不合法"))?;
        let schedule = WorkerSchedule::try_new(
            IQ_PROBE_INTERVAL,
            WORKER_INITIAL_BACKOFF,
            WORKER_MAXIMUM_BACKOFF,
            WORKER_LEASE_TTL,
            WORKER_LEASE_RENEWAL,
        )
        .map_err(|_| AdminError::internal("降智检测 Worker 调度配置不合法"))?;
        let lease = WorkerLeaseRequest::try_new(id.clone(), WORKER_LEASE_TTL)
            .map_err(|_| AdminError::internal("降智检测 Worker 租约配置不合法"))?;
        let registration = WorkerRegistration::try_new(
            id,
            WorkerRunnable::Scheduled {
                schedule,
                lease: Some(lease),
                task: Box::new(self),
            },
        )
        .map_err(|_| AdminError::internal("降智检测 Worker 注册信息不合法"))?;
        Ok(WorkerContribution::Registration(registration))
    }

    fn probes(&self) -> &dyn IqProbeStore {
        self.runner.deps.probes.as_ref()
    }

    /// 单个周期的检测逻辑；所有失败只记日志
    async fn run_cycle_inner(&self) {
        let now = Utc::now();
        // 清理不依赖开关，关闭检测后旧结果仍按保留期删除
        self.purge(now).await;
        let (settings, cursor) = match self.probes().load_settings().await {
            Ok(Some((settings, _, cursor))) => (settings, cursor),
            Ok(None) => (IqProbeSettings::default(), None),
            Err(error) => {
                warn!(error = %error, "读取降智检测设置失败");
                return;
            }
        };
        if !settings.enabled || settings.questions.is_empty() {
            return;
        }
        let Some(accounts) = self.normal_accounts().await else {
            return;
        };
        let mut schedule = match self.probes().list_schedule().await {
            Ok(rows) => rows
                .into_iter()
                .map(|row| (row.account_id.clone(), row))
                .collect::<HashMap<_, _>>(),
            Err(error) => {
                warn!(error = %error, "读取降智检测排期失败");
                return;
            }
        };
        self.assign_slots(&accounts, &mut schedule, cursor, &settings, now)
            .await;
        let settings = Arc::new(settings);
        for account_id in &accounts {
            if let Some(row) = schedule.get(account_id) {
                self.start_due_round(row, &settings, now).await;
            }
        }
    }

    async fn purge(&self, now: DateTime<Utc>) {
        if let Err(error) = self.probes().delete_orphan_schedule().await {
            warn!(error = %error, "清理已删除账号的降智检测排期失败");
        }
        let due = {
            let mut last = lock(&self.last_purge_at);
            if last.is_some_and(|at| {
                now - at < chrono::Duration::minutes(ROUND_PURGE_INTERVAL_MINUTES)
            }) {
                false
            } else {
                *last = Some(now);
                true
            }
        };
        if due
            && let Err(error) = self
                .probes()
                .delete_rounds_before(now - chrono::Duration::days(ROUND_RETENTION_DAYS))
                .await
        {
            warn!(error = %error, "清理过期降智检测结果失败");
        }
    }

    /// 按 id 升序列出状态为正常的账号
    ///
    /// 停用、额度用完或暂停、限流冷却、凭证异常都不是正常状态，自然跳过
    /// 限流状态依赖运行态快照，与账号列表页的判断一致
    async fn normal_accounts(&self) -> Option<Vec<String>> {
        let deps = &self.runner.deps;
        let runtime = match deps.runtime.active_rate_limits().await {
            Ok(runtime) => runtime,
            Err(error) => {
                warn!(error = %error, "读取账号限流状态失败，跳过本轮降智检测");
                return None;
            }
        };
        let page_size = PageSize::new(PageSize::MAX).ok()?;
        let mut ids = Vec::new();
        for page in 1_u32.. {
            let query = AccountListQuery {
                page,
                page_size,
                provider_kind: None,
                group_filter: None,
                search: None,
                status: Some(AccountStatus::Normal),
                // 不排序时存储按 id 升序返回，保证翻页稳定
                sort: None,
            };
            let result = match deps.store.list_accounts(query, runtime.clone()).await {
                Ok(result) => result,
                Err(error) => {
                    warn!(error = %error, "列出降智检测账号失败");
                    return None;
                }
            };
            let count = result.items.len();
            ids.extend(result.items.into_iter().map(|item| item.account.id));
            if count < usize::from(PageSize::MAX) {
                break;
            }
        }
        Some(ids)
    }

    /// 给还没有排期的账号按 id 顺序分配首次开测时间，相邻账号错开 `stagger_minutes`
    ///
    /// 游标记录最近分配出去的时间，新加入的账号排在队尾，不会和已排好的账号挤在一起
    async fn assign_slots(
        &self,
        accounts: &[String],
        schedule: &mut HashMap<String, IqProbeScheduleRow>,
        cursor: Option<DateTime<Utc>>,
        settings: &IqProbeSettings,
        now: DateTime<Utc>,
    ) {
        let stagger = chrono::Duration::minutes(i64::from(settings.stagger_minutes));
        let mut cursor = cursor;
        let mut moved = false;
        for account_id in accounts {
            if schedule.contains_key(account_id) {
                continue;
            }
            let slot = cursor.map_or(now, |at| (at + stagger).max(now));
            let row = IqProbeScheduleRow {
                account_id: account_id.clone(),
                next_due_at: slot,
                recheck_due_at: None,
                last_started_at: None,
            };
            if let Err(error) = self.probes().upsert_schedule(&row).await {
                warn!(account_id, error = %error, "写入降智检测排期失败");
                break;
            }
            schedule.insert(account_id.clone(), row);
            cursor = Some(slot);
            moved = true;
        }
        if moved && let Err(error) = self.probes().set_slot_cursor(cursor).await {
            warn!(error = %error, "更新降智检测排队游标失败");
        }
    }

    /// 到期就在后台开一轮；有待复测时只做复测，不叠加定时测试
    async fn start_due_round(
        &self,
        row: &IqProbeScheduleRow,
        settings: &Arc<IqProbeSettings>,
        now: DateTime<Utc>,
    ) {
        let kind = match row.recheck_due_at {
            Some(at) if at <= now => IqProbeKind::Recheck,
            Some(_) => return,
            None if row.next_due_at <= now => IqProbeKind::Scheduled,
            None => return,
        };
        let Some(guard) = RunningGuard::acquire(&self.running, &row.account_id) else {
            return;
        };
        if kind == IqProbeKind::Scheduled {
            // 下次时间按本次开测时间算，测得慢不推迟后面的轮次
            let next = IqProbeScheduleRow {
                next_due_at: now + chrono::Duration::minutes(i64::from(settings.interval_minutes)),
                last_started_at: Some(now),
                ..row.clone()
            };
            if let Err(error) = self.probes().upsert_schedule(&next).await {
                // 排期没写进去就不开测，避免下个周期重复开测
                warn!(account_id = row.account_id, error = %error, "更新降智检测排期失败");
                return;
            }
        }
        let runner = self.runner.clone();
        let settings = Arc::clone(settings);
        let account_id = row.account_id.clone();
        tokio::spawn(async move {
            let _guard = guard;
            runner.run_round(account_id, kind, &settings).await;
        });
    }
}

impl ScheduledTask for IqProbeTask {
    /// 任何情况都返回成功，任务报错会让健康检查变红
    fn run_cycle(
        &self,
        context: WorkerCycleContext,
    ) -> futures::future::BoxFuture<'_, Result<(), WorkerTaskError>> {
        Box::pin(async move {
            if context.cancellation().is_cancelled() {
                return Ok(());
            }
            self.run_cycle_inner().await;
            Ok(())
        })
    }
}

/// 在后台执行单轮考试与结束处理
#[derive(Clone)]
struct RoundRunner {
    deps: IqProbeWorkerDeps,
    /// 串行执行熔断计数、停号和落库，避免多个复测同时通过计数后超额停号
    disable_lock: Arc<tokio::sync::Mutex<()>>,
}

impl RoundRunner {
    async fn run_round(&self, account_id: String, kind: IqProbeKind, settings: &IqProbeSettings) {
        let Ok(account) = ProviderAccountId::new(account_id.clone()) else {
            warn!(account_id, "账号 ID 不合法，跳过降智检测");
            return;
        };
        let id = IqProbeRound::new_id();
        let started_at = Utc::now();
        // 一轮内所有题都问，不因前一题不及格提前结束，方便管理员看完整表现
        let mut questions = Vec::with_capacity(settings.questions.len());
        for question in &settings.questions {
            questions.push(self.ask_question(&account, question).await);
        }
        let verdicts = questions
            .iter()
            .map(|question| question.verdict)
            .collect::<Vec<_>>();
        let mut round = IqProbeRound {
            id,
            account_id,
            kind,
            started_at,
            finished_at: Utc::now(),
            outcome: round_outcome(&verdicts),
            action: IqProbeAction::None,
            detail: IqProbeRoundDetail { questions },
        };
        self.update_schedule(&round, settings).await;
        if kind == IqProbeKind::Recheck && round.outcome == IqProbeOutcome::Failed {
            let _lock = self.disable_lock.lock().await;
            round.action = self.disable_or_hold(&round, settings).await;
            self.insert_round(&round).await;
        } else {
            self.insert_round(&round).await;
        }
    }

    /// 定时轮不及格就安排复测；复测结束后清掉待复测，回到原定时节奏
    ///
    /// 没测成（出错）不算不及格，不安排复测
    async fn update_schedule(&self, round: &IqProbeRound, settings: &IqProbeSettings) {
        let recheck_due_at = match round.kind {
            IqProbeKind::Scheduled if round.outcome == IqProbeOutcome::Failed => Some(
                round.finished_at
                    + chrono::Duration::minutes(i64::from(settings.recheck_delay_minutes)),
            ),
            IqProbeKind::Scheduled => return,
            IqProbeKind::Recheck => None,
        };
        let rows = match self.deps.probes.list_schedule().await {
            Ok(rows) => rows,
            Err(error) => {
                warn!(account_id = round.account_id, error = %error, "读取降智检测排期失败");
                return;
            }
        };
        // 排期已被清空（重新开启检测）时不补写，账号会按新队列重新排
        let Some(row) = rows
            .into_iter()
            .find(|row| row.account_id == round.account_id)
        else {
            return;
        };
        let row = IqProbeScheduleRow {
            recheck_due_at,
            ..row
        };
        if let Err(error) = self.deps.probes.upsert_schedule(&row).await {
            warn!(account_id = round.account_id, error = %error, "更新降智检测复测时间失败");
        }
    }

    /// 复测仍不及格时停用账号；窗口内自动停号已达阈值时熔断，只标红不停号
    async fn disable_or_hold(
        &self,
        round: &IqProbeRound,
        settings: &IqProbeSettings,
    ) -> IqProbeAction {
        let since =
            Utc::now() - chrono::Duration::minutes(i64::from(IQ_PROBE_BREAKER_WINDOW_MINUTES));
        let disabled_in_window = match self
            .deps
            .probes
            .count_actions_since(IqProbeAction::Disabled, since)
            .await
        {
            Ok(count) => count,
            Err(error) => {
                // 无法确认熔断状态时不冒险停号，下一轮定时测试会再次判断
                warn!(account_id = round.account_id, error = %error, "读取降智检测熔断计数失败，本次不停号");
                return IqProbeAction::None;
            }
        };
        let (question, answer) = failed_evidence(round);
        if disabled_in_window >= settings.breaker_threshold {
            warn!(
                account_id = round.account_id,
                round_id = round.id,
                disabled_in_window,
                question,
                answer,
                "降智检测复测不及格，熔断生效未停号"
            );
            return IqProbeAction::BreakerHeld;
        }
        let context = MutationContext {
            actor: MutationActor::System,
            request_id: format!("iq-probe:{}", round.id),
        };
        let command = BatchUpdateAccounts {
            quota_settings: None,
            auto_use_quota_credits: None,
            account_ids: vec![round.account_id.clone()],
            enabled: Some(false),
            concurrency_limit: None,
            weight: None,
            model_access: None,
            group_ids: None,
            outbound_proxy: None,
        };
        match self.deps.accounts.batch_update(&context, command).await {
            Ok(_) => {
                warn!(
                    account_id = round.account_id,
                    round_id = round.id,
                    question,
                    answer,
                    "降智检测复测不及格，已停用账号"
                );
                IqProbeAction::Disabled
            }
            Err(error) => {
                // 账号可能已被删除或停用，只记日志
                warn!(
                    account_id = round.account_id,
                    round_id = round.id,
                    error = %error,
                    "降智检测停用账号失败"
                );
                IqProbeAction::None
            }
        }
    }

    async fn insert_round(&self, round: &IqProbeRound) {
        if let Err(error) = self.deps.probes.insert_round(round).await {
            warn!(account_id = round.account_id, round_id = round.id, error = %error, "写入降智检测结果失败");
        }
    }

    /// 一道题最多问 `max_attempts` 次，每次都是新对话；答对即停，出错不当成答错并停止该题
    async fn ask_question(
        &self,
        account: &ProviderAccountId,
        question: &IqProbeQuestion,
    ) -> IqProbeQuestionResult {
        let mut result = IqProbeQuestionResult {
            prompt: question.prompt.clone(),
            model: question.model.clone(),
            reasoning_effort: question.reasoning_effort.clone(),
            verdict: IqProbeQuestionVerdict::Failed,
            attempts: Vec::new(),
        };
        let Ok(model) = UpstreamModelId::new(question.model.clone()) else {
            result.verdict = IqProbeQuestionVerdict::Error;
            result.attempts.push(IqProbeAttempt {
                answer: String::new(),
                verdict: IqProbeAttemptVerdict::Error,
                error: Some("模型名不合法".to_owned()),
                elapsed_ms: 0,
            });
            return result;
        };
        for attempt_index in 0..question.max_attempts.max(1) {
            let attempt = self.ask_once(account, &model, question).await;
            let verdict = attempt.verdict;
            result.attempts.push(attempt);
            match verdict {
                IqProbeAttemptVerdict::Pass => {
                    result.verdict = if attempt_index == 0 {
                        IqProbeQuestionVerdict::Passed
                    } else {
                        IqProbeQuestionVerdict::PassedAfterRetry
                    };
                    break;
                }
                IqProbeAttemptVerdict::Error => {
                    result.verdict = IqProbeQuestionVerdict::Error;
                    break;
                }
                IqProbeAttemptVerdict::Fail | IqProbeAttemptVerdict::Unclear => {}
            }
        }
        result
    }

    async fn ask_once(
        &self,
        account: &ProviderAccountId,
        model: &UpstreamModelId,
        question: &IqProbeQuestion,
    ) -> IqProbeAttempt {
        let started = Instant::now();
        let answer = tokio::time::timeout(
            ATTEMPT_TIMEOUT,
            self.collect_answer(account, model, question),
        )
        .await
        .unwrap_or_else(|_| Err(format!("超过 {} 秒未完成", ATTEMPT_TIMEOUT.as_secs())));
        let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        match answer {
            Ok(answer) => IqProbeAttempt {
                verdict: judge(&answer, &question.pass_keywords, &question.fail_keywords),
                answer: truncate_chars(&answer, IQ_PROBE_ANSWER_MAX_CHARS),
                error: None,
                elapsed_ms,
            },
            Err(error) => IqProbeAttempt {
                answer: String::new(),
                verdict: IqProbeAttemptVerdict::Error,
                error: Some(error),
                elapsed_ms,
            },
        }
    }

    /// 拼接回答文本；收到失败事件或没有完成事件都算没拿到回答
    async fn collect_answer(
        &self,
        account: &ProviderAccountId,
        model: &UpstreamModelId,
        question: &IqProbeQuestion,
    ) -> Result<String, String> {
        let options = AccountConnectionTestOptions {
            input_text: Some(question.prompt.clone()),
            reasoning_effort: question.reasoning_effort.clone(),
        };
        // 用冻结恢复目标而不是连接测试：前者不回写额度头、失败分类等账号状态
        // 检测本身不应改变账号状态；该目标也不要求账号处于冻结中
        let mut events = self
            .deps
            .accounts
            .test_connection(
                AccountProbeTarget::FreezeRecovery(account.clone()),
                model.clone(),
                options,
            )
            .await
            .map_err(|error| truncate_chars(error.message(), ERROR_SUMMARY_CHARS))?;
        let mut answer = String::new();
        let mut completed = false;
        while let Some(event) = events.next().await {
            match event {
                AccountConnectionTestEvent::Content { text } => answer.push_str(&text),
                AccountConnectionTestEvent::Completed => completed = true,
                AccountConnectionTestEvent::Failed {
                    upstream_status,
                    message,
                    ..
                } => {
                    let summary = match upstream_status {
                        Some(status) => format!("上游 {status}：{message}"),
                        None => message,
                    };
                    return Err(truncate_chars(&summary, ERROR_SUMMARY_CHARS));
                }
                AccountConnectionTestEvent::Started { .. }
                | AccountConnectionTestEvent::Request { .. } => {}
            }
        }
        if completed {
            Ok(answer)
        } else {
            Err("上游未返回完成事件".to_owned())
        }
    }
}

/// 停号日志用的证据：第一道不及格的题和它最后一次回答的开头
fn failed_evidence(round: &IqProbeRound) -> (String, String) {
    round
        .detail
        .questions
        .iter()
        .find(|question| question.verdict == IqProbeQuestionVerdict::Failed)
        .map(|question| {
            let answer = question
                .attempts
                .last()
                .map(|attempt| truncate_chars(&attempt.answer, LOG_ANSWER_CHARS))
                .unwrap_or_default();
            (question.prompt.clone(), answer)
        })
        .unwrap_or_default()
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// 记录账号正在考试；考试任务结束（含异常退出）时自动移除
struct RunningGuard {
    running: Arc<Mutex<HashSet<String>>>,
    account_id: String,
}

impl RunningGuard {
    fn acquire(running: &Arc<Mutex<HashSet<String>>>, account_id: &str) -> Option<Self> {
        lock(running).insert(account_id.to_owned()).then(|| Self {
            running: Arc::clone(running),
            account_id: account_id.to_owned(),
        })
    }
}

impl Drop for RunningGuard {
    fn drop(&mut self) {
        lock(&self.running).remove(&self.account_id);
    }
}
