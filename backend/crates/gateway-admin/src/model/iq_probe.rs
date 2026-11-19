//! 降智检测（fork 自有功能）的设置、轮次结果与熔断状态
//!
//! 字段统一 camelCase，枚举值与 `fork_iq_probe_rounds` 表中的字符串一致

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 熔断窗口固定 120 分钟，只统计窗口内自动停号的次数
pub const IQ_PROBE_BREAKER_WINDOW_MINUTES: u32 = 120;
/// 每次回答最多保存的字符数，超出部分截断
pub const IQ_PROBE_ANSWER_MAX_CHARS: usize = 2000;
/// 账号列表每号展示的最近轮次数
pub const IQ_PROBE_RECENT_ROUNDS_PER_ACCOUNT: u32 = 24;
/// 一次批量查询结果最多允许的账号数
pub const IQ_PROBE_MAX_ROUND_ACCOUNTS: usize = 200;

const REASONING_EFFORTS: &[&str] = &["none", "minimal", "low", "medium", "high", "xhigh"];

/// 降智检测设置；库里没有设置行时使用 [`Default`]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IqProbeSettings {
    pub enabled: bool,
    pub interval_minutes: u32,
    pub stagger_minutes: u32,
    pub recheck_delay_minutes: u32,
    pub breaker_threshold: u32,
    pub questions: Vec<IqProbeQuestion>,
}

/// 一道检测题；关键词判定规则由后台任务执行
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IqProbeQuestion {
    pub prompt: String,
    pub model: String,
    pub reasoning_effort: Option<String>,
    pub pass_keywords: Vec<String>,
    pub fail_keywords: Vec<String>,
    pub max_attempts: u32,
}

impl Default for IqProbeSettings {
    /// 默认关闭，由管理员核对题目后再开启
    fn default() -> Self {
        Self {
            enabled: false,
            interval_minutes: 10,
            stagger_minutes: 2,
            recheck_delay_minutes: 2,
            breaker_threshold: 3,
            questions: vec![
                IqProbeQuestion {
                    prompt: "don't search the internet, do you know Thibault Sottiaux on X. answer yes or no".to_owned(),
                    model: "gpt-6-astra".to_owned(),
                    reasoning_effort: Some("high".to_owned()),
                    pass_keywords: vec!["yes".to_owned()],
                    fail_keywords: vec!["no".to_owned()],
                    max_attempts: 2,
                },
                IqProbeQuestion {
                    prompt: "叮咚鸡".to_owned(),
                    model: "gpt-6.2-sol".to_owned(),
                    reasoning_effort: Some("high".to_owned()),
                    pass_keywords: vec!["大狗".to_owned()],
                    fail_keywords: Vec::new(),
                    max_attempts: 3,
                },
            ],
        }
    }
}

impl IqProbeSettings {
    /// 去掉题目、模型和关键词两端空白，空推理强度视为未设置
    #[must_use]
    pub fn normalized(mut self) -> Self {
        for question in &mut self.questions {
            question.prompt = question.prompt.trim().to_owned();
            question.model = question.model.trim().to_owned();
            question.reasoning_effort = question
                .reasoning_effort
                .take()
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty());
            for keyword in question
                .pass_keywords
                .iter_mut()
                .chain(question.fail_keywords.iter_mut())
            {
                *keyword = keyword.trim().to_owned();
            }
        }
        self
    }

    /// 校验保存前的设置，返回可直接展示给管理员的中文原因
    ///
    /// # Errors
    ///
    /// 任一字段超出允许范围时返回原因
    pub fn validate(&self) -> Result<(), String> {
        check_range("每号间隔（分钟）", self.interval_minutes, 1, 1440)?;
        check_range("号间错开（分钟）", self.stagger_minutes, 0, 60)?;
        check_range("复测间隔（分钟）", self.recheck_delay_minutes, 1, 60)?;
        check_range("熔断阈值", self.breaker_threshold, 1, 100)?;
        if self.questions.is_empty() || self.questions.len() > 10 {
            return Err("题目数量需在 1 到 10 道之间".to_owned());
        }
        for (index, question) in self.questions.iter().enumerate() {
            question
                .validate()
                .map_err(|reason| format!("第 {} 题：{reason}", index + 1))?;
        }
        Ok(())
    }
}

impl IqProbeQuestion {
    fn validate(&self) -> Result<(), String> {
        let prompt = self.prompt.trim().chars().count();
        if prompt == 0 || prompt > 2000 {
            return Err("题目需为 1 到 2000 字".to_owned());
        }
        let model = self.model.trim().chars().count();
        if model == 0 || model > 128 {
            return Err("模型名需为 1 到 128 字".to_owned());
        }
        if let Some(effort) = self.reasoning_effort.as_deref().map(str::trim)
            && !effort.is_empty()
            && !REASONING_EFFORTS.contains(&effort)
        {
            return Err(format!("推理强度只能是 {}", REASONING_EFFORTS.join("、")));
        }
        check_keywords("算对关键词", &self.pass_keywords, 1)?;
        check_keywords("算错关键词", &self.fail_keywords, 0)?;
        check_range("最多问几次", self.max_attempts, 1, 5)
    }
}

fn check_range(label: &str, value: u32, min: u32, max: u32) -> Result<(), String> {
    if (min..=max).contains(&value) {
        Ok(())
    } else {
        Err(format!("{label}需在 {min} 到 {max} 之间"))
    }
}

fn check_keywords(label: &str, keywords: &[String], min: usize) -> Result<(), String> {
    if keywords.len() < min || keywords.len() > 10 {
        return Err(format!("{label}需为 {min} 到 10 个"));
    }
    if keywords.iter().any(|keyword| {
        let length = keyword.trim().chars().count();
        length == 0 || length > 64
    }) {
        return Err(format!("{label}每个需为 1 到 64 字"));
    }
    Ok(())
}

/// 轮次类型：按节奏的定时测试或不及格后的复测
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IqProbeKind {
    Scheduled,
    Recheck,
}

/// 一轮的总体结果
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IqProbeOutcome {
    Passed,
    PassedAfterRetry,
    Failed,
    Error,
}

/// 一轮结束后对账号采取的处置
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IqProbeAction {
    None,
    Disabled,
    BreakerHeld,
}

/// 单道题的结果，取值与轮次结果相同
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IqProbeQuestionVerdict {
    Passed,
    PassedAfterRetry,
    Failed,
    Error,
}

/// 单次提问的判定；`Error` 表示没拿到回答，不算答错
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IqProbeAttemptVerdict {
    Pass,
    Fail,
    Unclear,
    Error,
}

impl IqProbeKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Scheduled => "scheduled",
            Self::Recheck => "recheck",
        }
    }

    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "scheduled" => Some(Self::Scheduled),
            "recheck" => Some(Self::Recheck),
            _ => None,
        }
    }
}

impl IqProbeOutcome {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::PassedAfterRetry => "passed_after_retry",
            Self::Failed => "failed",
            Self::Error => "error",
        }
    }

    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "passed" => Some(Self::Passed),
            "passed_after_retry" => Some(Self::PassedAfterRetry),
            "failed" => Some(Self::Failed),
            "error" => Some(Self::Error),
            _ => None,
        }
    }
}

impl IqProbeAction {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Disabled => "disabled",
            Self::BreakerHeld => "breaker_held",
        }
    }

    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "none" => Some(Self::None),
            "disabled" => Some(Self::Disabled),
            "breaker_held" => Some(Self::BreakerHeld),
            _ => None,
        }
    }
}

/// 一个账号的一轮检测记录
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IqProbeRound {
    pub id: String,
    pub account_id: String,
    pub kind: IqProbeKind,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub outcome: IqProbeOutcome,
    pub action: IqProbeAction,
    pub detail: IqProbeRoundDetail,
}

impl IqProbeRound {
    /// 轮次 ID 由调用方生成，停号审计的 request_id 需要在落库前引用它
    #[must_use]
    pub fn new_id() -> String {
        format!("iqr_{}", uuid::Uuid::now_v7().simple())
    }
}

/// 一轮里每道题的明细，按设置中的题目顺序
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IqProbeRoundDetail {
    pub questions: Vec<IqProbeQuestionResult>,
}

/// 单道题的题面快照与每次提问记录
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IqProbeQuestionResult {
    pub prompt: String,
    pub model: String,
    pub reasoning_effort: Option<String>,
    pub verdict: IqProbeQuestionVerdict,
    pub attempts: Vec<IqProbeAttempt>,
}

/// 单次提问；回答最多保存 [`IQ_PROBE_ANSWER_MAX_CHARS`] 个字符
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IqProbeAttempt {
    pub answer: String,
    pub verdict: IqProbeAttemptVerdict,
    /// 没拿到回答时的原因摘要，含上游状态码
    pub error: Option<String>,
    pub elapsed_ms: u64,
}

/// 账号排期；`recheck_due_at` 有值时优先复测
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IqProbeScheduleRow {
    pub account_id: String,
    pub next_due_at: DateTime<Utc>,
    pub recheck_due_at: Option<DateTime<Utc>>,
    pub last_started_at: Option<DateTime<Utc>>,
}

/// 熔断状态：窗口内自动停号数达到阈值时只标红不停号
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IqProbeStatus {
    pub breaker_active: bool,
    pub auto_disabled_in_window: u32,
    pub window_minutes: u32,
}

/// 设置页读取与保存后返回的完整视图；没有设置行时 revision 为 0
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IqProbeSettingsView {
    pub settings: IqProbeSettings,
    pub revision: u64,
    pub status: IqProbeStatus,
}
