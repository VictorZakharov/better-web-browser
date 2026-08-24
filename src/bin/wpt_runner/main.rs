mod browser;
mod cli;
mod manifest;
mod report;
mod server;

use cli::Cli;
use manifest::Manifest;
use report::{CaseResult, RunConfiguration, RunReport};
use server::TestServer;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

fn main() {
    let exit_code = match run() {
        Ok(true) => 0,
        Ok(false) => 1,
        Err(error) => {
            eprintln!("wpt-runner: {error}");
            2
        }
    };
    if exit_code != 0 {
        std::process::exit(exit_code);
    }
}

fn run() -> Result<bool, String> {
    let Some(cli) = Cli::parse()? else {
        return Ok(true);
    };
    if !cli.browser.is_file() {
        return Err(format!(
            "Breeze executable is missing: {}; build the selected profile first",
            cli.browser.display()
        ));
    }
    let manifest = Manifest::load(&cli.manifest)?;
    let tests = manifest.selected(cli.filter.as_deref())?;
    let minimum_subtests = if cli.filter.is_none() {
        manifest.policy.minimum_subtests
    } else {
        1
    };
    let root = manifest.verify_checkout(&cli.wpt_root, &tests)?;
    let server = TestServer::start(root, tests.clone())?;

    println!(
        "Running {} WPT cases for {:?} from {}",
        tests.len(),
        manifest.suite,
        &manifest.upstream.revision[..12]
    );
    let results = execute_tests(&cli, &server, tests)?;

    let report = RunReport::new(
        manifest.suite,
        manifest.upstream,
        RunConfiguration {
            browser: &cli.browser,
            manifest: &cli.manifest,
            settle_ms: cli.settle_ms,
            timeout_ms: cli.timeout_ms,
            jobs: cli.jobs,
        },
        minimum_subtests,
        results,
    );
    report.write(&cli.output)?;
    println!(
        "Summary: {} pass, {} expected failure, {} unexpected pass, {} regression ({} total)",
        report.summary.passed,
        report.summary.expected_failures,
        report.summary.unexpected_passes,
        report.summary.regressions,
        report.summary.total
    );
    println!(
        "Harness subtests: {} pass, {} fail, {} timeout ({} total, {} required)",
        report.summary.subtests_passed,
        report.summary.subtests_failed,
        report.summary.subtests_timed_out,
        report.summary.subtests_total,
        report.summary.minimum_subtests,
    );
    println!("JSON report: {}", cli.output.display());
    Ok(report.summary.is_success())
}

fn execute_tests(
    cli: &Cli,
    server: &TestServer,
    tests: Vec<manifest::TestCase>,
) -> Result<Vec<CaseResult>, String> {
    let total = tests.len();
    let urls = (0..total)
        .map(|index| server.url_for(index))
        .collect::<Vec<_>>();
    let next = AtomicUsize::new(0);
    let results = (0..total)
        .map(|_| Mutex::new(None))
        .collect::<Vec<Mutex<Option<CaseResult>>>>();

    std::thread::scope(|scope| -> Result<(), String> {
        let mut workers = Vec::new();
        for _ in 0..cli.jobs.min(total) {
            workers.push(scope.spawn(|| -> Result<(), String> {
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    if index >= total {
                        return Ok(());
                    }
                    let observation = browser::run(
                        &cli.browser,
                        &urls[index],
                        cli.settle_ms,
                        cli.timeout_ms,
                    );
                    let result = CaseResult::from_observation(tests[index].clone(), observation);
                    println!(
                        "[{current:>2}/{total}] {verdict:<16} {path} (expected {expected}, actual {actual}, {duration} ms)",
                        current = index + 1,
                        verdict = result.verdict,
                        path = result.path,
                        expected = result.expected,
                        actual = result.actual,
                        duration = result.duration_ms,
                    );
                    *results[index]
                        .lock()
                        .map_err(|_| "WPT result slot is unavailable".to_string())? = Some(result);
                }
            }));
        }
        for worker in workers {
            worker
                .join()
                .map_err(|_| "WPT execution worker panicked".to_string())??;
        }
        Ok(())
    })?;

    let server_errors = server.drain_errors();
    if !server_errors.is_empty() {
        return Err(format!(
            "local WPT server failed: {}",
            server_errors.join("; ")
        ));
    }
    results
        .into_iter()
        .map(|result| {
            result
                .into_inner()
                .map_err(|_| "WPT result slot is unavailable".to_string())?
                .ok_or_else(|| "WPT execution worker omitted a result".to_string())
        })
        .collect()
}
