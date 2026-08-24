use crate::report::{ActualStatus, HarnessReport, Observation, RESULT_MARKER};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
static NEXT_ARTIFACT_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Deserialize)]
struct BenchmarkReport {
    error: Option<String>,
    http_status: u32,
    #[serde(default)]
    javascript_errors: Vec<String>,
    #[serde(default)]
    javascript_console: Vec<String>,
    #[serde(default)]
    javascript_diagnostics: Vec<String>,
    #[serde(default)]
    javascript_runtime_stopped: bool,
}

pub(crate) fn run(browser: &Path, url: &str, settle_ms: u64, timeout_ms: u64) -> Observation {
    let started = Instant::now();
    let artifacts = match Artifacts::new() {
        Ok(artifacts) => artifacts,
        Err(error) => return crash(started, error),
    };
    let stderr_file = match std::fs::File::create(&artifacts.stderr) {
        Ok(file) => file,
        Err(error) => return crash(started, format!("create process stderr capture: {error}")),
    };
    let mut command = Command::new(browser);
    command
        .args([
            "--benchmark",
            url,
            "--output",
            path_text(&artifacts.report),
            "--settle-ms",
            &settle_ms.to_string(),
            "--completion-marker",
            RESULT_MARKER,
        ])
        .env("BREEZE_HEADLESS_LARGE_STACK", "1")
        .stdout(Stdio::null())
        .stderr(Stdio::from(stderr_file));
    configure_hidden(&mut command);

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            return crash(
                started,
                format!(
                    "launch hidden Breeze process {}: {error}",
                    browser.display()
                ),
            );
        }
    };
    let status = match wait_for_child(&mut child, Duration::from_millis(timeout_ms)) {
        Ok(status) => status,
        Err(error) => {
            return Observation {
                actual: ActualStatus::Timeout,
                duration_ms: started.elapsed().as_millis(),
                detail: Some(error),
                harness: None,
                javascript_errors: Vec::new(),
                javascript_diagnostics: Vec::new(),
                process_stderr: read_stderr(&artifacts.stderr),
            };
        }
    };
    let process_stderr = read_stderr(&artifacts.stderr);
    if !status.success() {
        return crash_with_stderr(
            started,
            format!("Breeze exited with {}", status_text(status.code())),
            process_stderr,
        );
    }
    if process_stderr.as_deref().is_some_and(contains_panic) {
        return crash_with_stderr(
            started,
            "a Breeze thread panicked despite a successful process exit".to_string(),
            process_stderr,
        );
    }

    let report_source = match std::fs::read_to_string(&artifacts.report) {
        Ok(report) => report,
        Err(error) => return crash(started, format!("read Breeze benchmark report: {error}")),
    };
    let benchmark: BenchmarkReport = match serde_json::from_str(&report_source) {
        Ok(report) => report,
        Err(error) => return crash(started, format!("parse Breeze benchmark report: {error}")),
    };
    observe_benchmark(started, benchmark, process_stderr)
}

fn observe_benchmark(
    started: Instant,
    benchmark: BenchmarkReport,
    process_stderr: Option<String>,
) -> Observation {
    let harness = match harness_report(&benchmark.javascript_console) {
        Ok(harness) => harness,
        Err(detail) => {
            return Observation {
                actual: ActualStatus::Crash,
                duration_ms: started.elapsed().as_millis(),
                detail: Some(detail),
                harness: None,
                javascript_errors: benchmark.javascript_errors,
                javascript_diagnostics: benchmark.javascript_diagnostics,
                process_stderr,
            };
        }
    };
    let (actual, detail) = if let Some(harness) = &harness {
        (harness.actual_status(), benchmark.error.clone())
    } else if benchmark.http_status >= 400 || benchmark.error.is_some() {
        (
            ActualStatus::Fail,
            benchmark
                .error
                .clone()
                .or_else(|| Some(format!("HTTP status {}", benchmark.http_status))),
        )
    } else if !benchmark.javascript_errors.is_empty() || benchmark.javascript_runtime_stopped {
        (
            ActualStatus::Fail,
            Some("testharness.js did not report completion after a script error".to_string()),
        )
    } else {
        (
            ActualStatus::Timeout,
            Some("testharness.js did not report completion before the settle deadline".to_string()),
        )
    };

    Observation {
        actual,
        duration_ms: started.elapsed().as_millis(),
        detail,
        harness,
        javascript_errors: benchmark.javascript_errors,
        javascript_diagnostics: benchmark.javascript_diagnostics,
        process_stderr,
    }
}

fn harness_report(console: &[String]) -> Result<Option<HarnessReport>, String> {
    let Some(payload) = console.iter().rev().find_map(|line| {
        line.find(RESULT_MARKER)
            .map(|index| &line[index + RESULT_MARKER.len()..])
    }) else {
        return Ok(None);
    };
    serde_json::from_str(payload)
        .map(Some)
        .map_err(|error| format!("parse testharness callback report: {error}"))
}

fn wait_for_child(
    child: &mut Child,
    timeout: Duration,
) -> Result<std::process::ExitStatus, String> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("poll hidden Breeze process: {error}"))?
        {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            child
                .kill()
                .map_err(|error| format!("terminate timed-out Breeze process: {error}"))?;
            let _ = child.wait();
            return Err(format!("hidden Breeze process exceeded {timeout:?}"));
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

#[cfg(windows)]
fn configure_hidden(command: &mut Command) {
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn configure_hidden(_command: &mut Command) {}

fn crash(started: Instant, detail: String) -> Observation {
    crash_with_stderr(started, detail, None)
}

fn crash_with_stderr(
    started: Instant,
    detail: String,
    process_stderr: Option<String>,
) -> Observation {
    Observation {
        actual: ActualStatus::Crash,
        duration_ms: started.elapsed().as_millis(),
        detail: Some(detail),
        harness: None,
        javascript_errors: Vec::new(),
        javascript_diagnostics: Vec::new(),
        process_stderr,
    }
}

fn read_stderr(path: &Path) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|stderr| stderr.trim().to_string())
        .filter(|stderr| !stderr.is_empty())
}

fn contains_panic(stderr: &str) -> bool {
    stderr.contains("panicked at") || stderr.contains("panicked after")
}

fn status_text(code: Option<i32>) -> String {
    code.map_or_else(
        || "an unknown status".to_string(),
        |code| format!("status {code}"),
    )
}

fn path_text(path: &Path) -> &str {
    path.to_str().expect("temporary path is valid UTF-8")
}

struct Artifacts {
    root: PathBuf,
    report: PathBuf,
    stderr: PathBuf,
}

impl Artifacts {
    fn new() -> Result<Self, String> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let id = NEXT_ARTIFACT_ID.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("breeze-wpt-{}-{nonce}-{id}", std::process::id()));
        std::fs::create_dir(&root)
            .map_err(|error| format!("create temporary WPT artifact directory: {error}"))?;
        Ok(Self {
            report: root.join("benchmark.json"),
            stderr: root.join("stderr.txt"),
            root,
        })
    }
}

impl Drop for Artifacts {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_marker_after_the_console_level_prefix() {
        let report = harness_report(&[format!(
            "log: {RESULT_MARKER}{{\"overall\":{{\"status\":\"OK\"}},\"tests\":[]}}"
        )])
        .unwrap()
        .unwrap();
        assert_eq!(report.overall.status, "OK");
    }

    #[test]
    fn rejects_a_corrupt_marker_instead_of_reporting_a_timeout() {
        assert!(harness_report(&[format!("log: {RESULT_MARKER}not-json")]).is_err());
    }
}
