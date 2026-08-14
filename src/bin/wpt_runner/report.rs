use crate::manifest::{ExpectedStatus, TestCase, Upstream};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) const RESULT_MARKER: &str = "__BREEZE_WPT_RESULT__";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ActualStatus {
    Pass,
    Fail,
    Timeout,
    Crash,
}

impl std::fmt::Display for ActualStatus {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::Timeout => "timeout",
            Self::Crash => "crash",
        };
        formatter.write_str(name)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Verdict {
    Pass,
    ExpectedFailure,
    UnexpectedPass,
    Regression,
}

impl std::fmt::Display for Verdict {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::Pass => "PASS",
            Self::ExpectedFailure => "EXPECTED FAILURE",
            Self::UnexpectedPass => "UNEXPECTED PASS",
            Self::Regression => "REGRESSION",
        };
        formatter.write_str(name)
    }
}

pub(crate) fn evaluate(expected: ExpectedStatus, actual: ActualStatus) -> Verdict {
    match (expected, actual) {
        (ExpectedStatus::Pass, ActualStatus::Pass) => Verdict::Pass,
        (ExpectedStatus::Pass, _) => Verdict::Regression,
        (ExpectedStatus::Fail, ActualStatus::Fail)
        | (ExpectedStatus::Timeout, ActualStatus::Timeout) => Verdict::ExpectedFailure,
        (_, ActualStatus::Pass) => Verdict::UnexpectedPass,
        _ => Verdict::Regression,
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct HarnessStatus {
    pub(crate) status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) message: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct HarnessTest {
    pub(crate) name: String,
    pub(crate) status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) stack: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct HarnessReport {
    pub(crate) overall: HarnessStatus,
    pub(crate) tests: Vec<HarnessTest>,
}

impl HarnessReport {
    pub(crate) fn actual_status(&self) -> ActualStatus {
        if self.overall.status == "TIMEOUT"
            || self.tests.iter().any(|test| test.status == "TIMEOUT")
        {
            return ActualStatus::Timeout;
        }
        if self.overall.status == "OK"
            && !self.tests.is_empty()
            && self.tests.iter().all(|test| test.status == "PASS")
        {
            ActualStatus::Pass
        } else {
            ActualStatus::Fail
        }
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct CaseResult {
    pub(crate) path: String,
    pub(crate) area: String,
    pub(crate) expected: ExpectedStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) expectation_reason: Option<String>,
    pub(crate) actual: ActualStatus,
    pub(crate) verdict: Verdict,
    pub(crate) duration_ms: u128,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) harness: Option<HarnessReport>,
    pub(crate) javascript_errors: Vec<String>,
    pub(crate) javascript_diagnostics: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) process_stderr: Option<String>,
}

impl CaseResult {
    pub(crate) fn from_observation(test: TestCase, observation: Observation) -> Self {
        let verdict = evaluate(test.expected, observation.actual);
        Self {
            path: test.path,
            area: test.area,
            expected: test.expected,
            expectation_reason: test.reason,
            actual: observation.actual,
            verdict,
            duration_ms: observation.duration_ms,
            detail: observation.detail,
            harness: observation.harness,
            javascript_errors: observation.javascript_errors,
            javascript_diagnostics: observation.javascript_diagnostics,
            process_stderr: observation.process_stderr,
        }
    }
}

pub(crate) struct Observation {
    pub(crate) actual: ActualStatus,
    pub(crate) duration_ms: u128,
    pub(crate) detail: Option<String>,
    pub(crate) harness: Option<HarnessReport>,
    pub(crate) javascript_errors: Vec<String>,
    pub(crate) javascript_diagnostics: Vec<String>,
    pub(crate) process_stderr: Option<String>,
}

#[derive(Debug, Default, Serialize)]
pub(crate) struct Summary {
    pub(crate) total: usize,
    pub(crate) passed: usize,
    pub(crate) expected_failures: usize,
    pub(crate) unexpected_passes: usize,
    pub(crate) regressions: usize,
    pub(crate) actual_timeouts: usize,
    pub(crate) actual_crashes: usize,
}

impl Summary {
    fn from_results(results: &[CaseResult]) -> Self {
        let mut summary = Self {
            total: results.len(),
            ..Self::default()
        };
        for result in results {
            match result.verdict {
                Verdict::Pass => summary.passed += 1,
                Verdict::ExpectedFailure => summary.expected_failures += 1,
                Verdict::UnexpectedPass => summary.unexpected_passes += 1,
                Verdict::Regression => summary.regressions += 1,
            }
            if result.actual == ActualStatus::Timeout {
                summary.actual_timeouts += 1;
            }
            if result.actual == ActualStatus::Crash {
                summary.actual_crashes += 1;
            }
        }
        summary
    }

    pub(crate) fn is_success(&self) -> bool {
        self.unexpected_passes == 0 && self.regressions == 0
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct RunReport {
    schema: u32,
    suite: String,
    generated_at_unix_ms: u128,
    upstream: Upstream,
    browser: String,
    manifest: String,
    execution_context: String,
    settle_ms: u64,
    timeout_ms: u64,
    pub(crate) summary: Summary,
    tests: Vec<CaseResult>,
}

impl RunReport {
    pub(crate) fn new(
        suite: String,
        upstream: Upstream,
        browser: &Path,
        manifest: &Path,
        settle_ms: u64,
        timeout_ms: u64,
        tests: Vec<CaseResult>,
    ) -> Self {
        Self {
            schema: 1,
            suite,
            generated_at_unix_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
            upstream,
            browser: browser.display().to_string(),
            manifest: manifest.display().to_string(),
            execution_context: "window".to_string(),
            settle_ms,
            timeout_ms,
            summary: Summary::from_results(&tests),
            tests,
        }
    }

    pub(crate) fn write(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent).map_err(|error| {
                format!("create report directory {}: {error}", parent.display())
            })?;
        }
        let json = serde_json::to_vec_pretty(self)
            .map_err(|error| format!("serialize WPT report: {error}"))?;
        std::fs::write(path, json)
            .map_err(|error| format!("write WPT report {}: {error}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expected_failures_are_precise_and_crashes_are_never_blessed() {
        assert_eq!(
            evaluate(ExpectedStatus::Fail, ActualStatus::Fail),
            Verdict::ExpectedFailure
        );
        assert_eq!(
            evaluate(ExpectedStatus::Fail, ActualStatus::Pass),
            Verdict::UnexpectedPass
        );
        assert_eq!(
            evaluate(ExpectedStatus::Fail, ActualStatus::Crash),
            Verdict::Regression
        );
        assert_eq!(
            evaluate(ExpectedStatus::Timeout, ActualStatus::Fail),
            Verdict::Regression
        );
    }
}
