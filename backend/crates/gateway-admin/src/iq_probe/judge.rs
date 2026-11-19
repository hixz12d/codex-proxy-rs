//! 按关键词判定单次回答，并汇总单题与整轮结果
//!
//! 只做纯函数判定，不访问存储或上游

use crate::model::iq_probe::{IqProbeAttemptVerdict, IqProbeOutcome, IqProbeQuestionVerdict};

/// 命中算对关键词且没命中算错关键词为对，反之为错；两者都命中或都没命中为不确定
pub(crate) fn judge(
    answer: &str,
    pass_keywords: &[String],
    fail_keywords: &[String],
) -> IqProbeAttemptVerdict {
    let answer = answer.to_lowercase();
    let pass = pass_keywords
        .iter()
        .any(|keyword| contains_keyword(&answer, keyword));
    let fail = fail_keywords
        .iter()
        .any(|keyword| contains_keyword(&answer, keyword));
    match (pass, fail) {
        (true, false) => IqProbeAttemptVerdict::Pass,
        (false, true) => IqProbeAttemptVerdict::Fail,
        _ => IqProbeAttemptVerdict::Unclear,
    }
}

/// 整轮结果：不及格优先于没测成，没测成优先于追问后通过
pub(crate) fn round_outcome(verdicts: &[IqProbeQuestionVerdict]) -> IqProbeOutcome {
    if verdicts.contains(&IqProbeQuestionVerdict::Failed) {
        IqProbeOutcome::Failed
    } else if verdicts.contains(&IqProbeQuestionVerdict::Error) {
        IqProbeOutcome::Error
    } else if verdicts.contains(&IqProbeQuestionVerdict::PassedAfterRetry) {
        IqProbeOutcome::PassedAfterRetry
    } else {
        IqProbeOutcome::Passed
    }
}

/// `answer` 已转小写
///
/// 纯英文数字关键词按整词匹配，避免 `know` 命中 `no`
/// 含中文、空格或符号的关键词按包含匹配，中文没有词边界
fn contains_keyword(answer: &str, keyword: &str) -> bool {
    let keyword = keyword.trim().to_lowercase();
    if keyword.is_empty() {
        return false;
    }
    if !keyword.chars().all(|ch| ch.is_ascii_alphanumeric()) {
        return answer.contains(&keyword);
    }
    answer.match_indices(&keyword).any(|(start, matched)| {
        let before = answer[..start].chars().next_back();
        let after = answer[start + matched.len()..].chars().next();
        !before.is_some_and(|ch| ch.is_ascii_alphanumeric())
            && !after.is_some_and(|ch| ch.is_ascii_alphanumeric())
    })
}
