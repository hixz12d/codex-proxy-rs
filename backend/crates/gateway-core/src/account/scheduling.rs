//! 账号调度参数与共享重置窗口评分；固定十分位保证快照可精确比较，浮点数只用于 wire 与评分

use std::time::SystemTime;

use super::AccountRuntimeSignals;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(into = "SmartSchedulingValues")]
pub struct SmartSchedulingConfig {
    weights: [u8; 6],
    reset_windows: ResetWindowWeights,
    // 老账号优先系数单独保存，保持原有六项的顺序与接口不变
    age: u8,
}

/// 账号通用额度的周期；Provider 按上游周期事实投影，不依赖窗口名称和槽位
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuotaResetPeriod {
    FiveHour,
    Weekly,
    Monthly,
}

impl QuotaResetPeriod {
    const fn seconds(self) -> u32 {
        match self {
            Self::FiveHour => 18_000,
            Self::Weekly => 604_800,
            Self::Monthly => 2_592_000,
        }
    }
}

fn weight_tenths(value: f64, field: &'static str) -> Result<u8, SchedulingConfigError> {
    if !(0.0..=10.0).contains(&value) || (value * 10.0).round() / 10.0 != value {
        return Err(SchedulingConfigError::InvalidWeight(field));
    }
    Ok((value * 10.0).round() as u8)
}

impl<'de> Deserialize<'de> for SmartSchedulingConfig {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // flatten 会缓冲字段，arbitrary_precision 的小数需先恢复为 JSON Number
        // 随后仍由同一类型检查完整字段、数值范围和十分位精度
        let value = serde_json::Value::deserialize(deserializer)?;
        let values = serde_json::from_value::<SmartSchedulingValues>(value)
            .map_err(serde::de::Error::custom)?;
        Self::try_from(values).map_err(serde::de::Error::custom)
    }
}

impl Default for SmartSchedulingConfig {
    fn default() -> Self {
        Self {
            weights: [10, 8, 10, 5, 0, 0],
            reset_windows: ResetWindowWeights::default(),
            age: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SchedulingConfigError {
    #[error("{0} must be between 0 and 10 with at most one decimal place")]
    InvalidWeight(&'static str),
    #[error("at least one smart scheduling weight must be positive")]
    EmptyWeights,
}

impl SmartSchedulingConfig {
    /// 顺序为负载、剩余额度、健康、首输出延迟、额度重置和排队压力
    pub fn new(weights: [f64; 6]) -> Result<Self, SchedulingConfigError> {
        Self::with_age_weight(weights, 0.0)
    }

    /// 在六项系数之外附带老账号优先系数，0 表示关闭
    pub fn with_age_weight(
        weights: [f64; 6],
        age_weight: f64,
    ) -> Result<Self, SchedulingConfigError> {
        let mut tenths = [0; 6];
        for (index, field) in [
            "loadWeight",
            "quotaWeight",
            "healthWeight",
            "latencyWeight",
            "resetWeight",
            "queueWeight",
        ]
        .into_iter()
        .enumerate()
        {
            tenths[index] = weight_tenths(weights[index], field)?;
        }
        let age = weight_tenths(age_weight, "ageWeight")?;
        if tenths == [0; 6] && age == 0 {
            return Err(SchedulingConfigError::EmptyWeights);
        }
        Ok(Self {
            weights: tenths,
            age,
            ..Self::default()
        })
    }

    #[must_use]
    pub fn weights(self) -> [f64; 6] {
        self.weights.map(|weight| f64::from(weight) / 10.0)
    }

    /// 老账号优先系数；只作用于智能策略下新会话的首轮选号
    #[must_use]
    pub fn age_weight(self) -> f64 {
        f64::from(self.age) / 10.0
    }

    fn reset_enabled(self) -> bool {
        self.weights[4] > 0 && self.reset_windows.enabled()
    }

    pub(crate) fn reset_score(self, signals: &AccountRuntimeSignals, now: SystemTime) -> f64 {
        if !self.reset_enabled() {
            return 0.0;
        }
        self.reset_windows.score(signals, now)
    }

    pub(crate) fn score_tolerance(self) -> f64 {
        // 与系数同步缩放，避免仅放大相同比例的系数就改变近似最优候选集合
        // 排队和未启用的重置窗口不能放大立即选号的容差
        // 老账号加分参与立即选号，同样计入；为 0 时与不计入完全相同
        let count = if self.reset_enabled() { 5 } else { 4 };
        0.05 * f64::from(
            self.weights[..count]
                .iter()
                .copied()
                .map(u16::from)
                .sum::<u16>()
                + u16::from(self.age),
        ) / 33.0
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SmartSchedulingValues {
    load_weight: f64,
    quota_weight: f64,
    health_weight: f64,
    latency_weight: f64,
    reset_weight: f64,
    queue_weight: f64,
    #[serde(default)]
    reset_window_weights: ResetWindowWeights,
    // 缺省按 0 读取，为 0 时不输出，关闭时存储与接口 JSON 保持原有字段集合
    #[serde(default, skip_serializing_if = "Option::is_none")]
    age_weight: Option<f64>,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ResetWindowValues {
    five_hour: f64,
    weekly: f64,
    monthly: f64,
}

impl TryFrom<SmartSchedulingValues> for SmartSchedulingConfig {
    type Error = SchedulingConfigError;

    fn try_from(value: SmartSchedulingValues) -> Result<Self, Self::Error> {
        let mut config = Self::with_age_weight(
            [
                value.load_weight,
                value.quota_weight,
                value.health_weight,
                value.latency_weight,
                value.reset_weight,
                value.queue_weight,
            ],
            value.age_weight.unwrap_or_default(),
        )?;
        config.reset_windows = value.reset_window_weights;
        Ok(config)
    }
}

impl From<SmartSchedulingConfig> for SmartSchedulingValues {
    fn from(value: SmartSchedulingConfig) -> Self {
        let [
            load_weight,
            quota_weight,
            health_weight,
            latency_weight,
            reset_weight,
            queue_weight,
        ] = value.weights();
        Self {
            load_weight,
            quota_weight,
            health_weight,
            latency_weight,
            reset_weight,
            queue_weight,
            reset_window_weights: value.reset_windows,
            age_weight: (value.age > 0).then(|| value.age_weight()),
        }
    }
}

/// 两种调度策略共用的重置窗口系数，各策略独立持有配置
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(into = "ResetWindowValues")]
pub struct ResetWindowWeights {
    weights: [u8; 3],
}

impl ResetWindowWeights {
    /// 顺序为 5 小时、周、月；全零表示未指定窗口
    pub fn new(weights: [f64; 3]) -> Result<Self, SchedulingConfigError> {
        Ok(Self {
            weights: [
                weight_tenths(weights[0], "fiveHour")?,
                weight_tenths(weights[1], "weekly")?,
                weight_tenths(weights[2], "monthly")?,
            ],
        })
    }

    pub(crate) fn enabled(self) -> bool {
        self.weights != [0; 3]
    }

    pub(crate) fn score(self, signals: &AccountRuntimeSignals, now: SystemTime) -> f64 {
        if !self.enabled() {
            return 0.0;
        }
        let mut scores = [0.0_f64; 3];
        for window in signals.quota_reset_windows.as_deref().unwrap_or_default() {
            let Ok(remaining) = window.reset_at.duration_since(now) else {
                continue;
            };
            if remaining.is_zero() {
                continue;
            }
            let half = f64::from(window.period.seconds()) / 7.0;
            let score = half / (half + remaining.as_secs_f64());
            // 同一周期只计最近的有效重置，避免重复额度桶叠加得分
            let slot = &mut scores[window.period as usize];
            *slot = slot.max(score);
        }
        // 缺失周期得零分，分母固定，避免少报窗口反而提高排名
        let weights = self.weights.map(f64::from);
        scores
            .into_iter()
            .zip(weights)
            .map(|(score, w)| score * w)
            .sum::<f64>()
            / weights.into_iter().sum::<f64>()
    }
}

impl<'de> Deserialize<'de> for ResetWindowWeights {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // 与智能调度一致，先恢复 flatten 缓冲的小数再校验
        let value = serde_json::Value::deserialize(deserializer)?;
        let values =
            serde_json::from_value::<ResetWindowValues>(value).map_err(serde::de::Error::custom)?;
        Self::new([values.five_hour, values.weekly, values.monthly])
            .map_err(serde::de::Error::custom)
    }
}

impl From<ResetWindowWeights> for ResetWindowValues {
    fn from(value: ResetWindowWeights) -> Self {
        let [five_hour, weekly, monthly] = value.weights.map(|weight| f64::from(weight) / 10.0);
        Self {
            five_hour,
            weekly,
            monthly,
        }
    }
}
