mod browser;
mod cli;
mod manifest;
mod report;
mod server;

use cli::Cli;
use manifest::Manifest;
use report::{CaseResult, RunReport};
use server::TestServer;

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
            "Breeze executable is missing: {}; build the release binaries first",
            cli.browser.display()
        ));
    }
    let manifest = Manifest::load(&cli.manifest)?;
    let tests = manifest.selected(cli.filter.as_deref())?;
    let total = tests.len();
    let root = manifest.verify_checkout(&cli.wpt_root, &tests)?;
    let server = TestServer::start(root, tests.clone())?;

    println!(
        "Running {} curated WPT cases from {}",
        tests.len(),
        &manifest.upstream.revision[..12]
    );
    let mut results = Vec::with_capacity(tests.len());
    for (index, test) in tests.into_iter().enumerate() {
        let url = server.url_for(index);
        let mut observation = browser::run(&cli.browser, &url, cli.settle_ms, cli.timeout_ms);
        let server_errors = server.drain_errors();
        if !server_errors.is_empty() {
            observation.actual = report::ActualStatus::Crash;
            observation.detail = Some(format!(
                "local WPT server failed: {}",
                server_errors.join("; ")
            ));
        }
        let result = CaseResult::from_observation(test, observation);
        println!(
            "[{current:>2}/{total}] {verdict:<16} {path} (expected {expected}, actual {actual}, {duration} ms)",
            current = index + 1,
            total = total,
            verdict = result.verdict,
            path = result.path,
            expected = result.expected,
            actual = result.actual,
            duration = result.duration_ms,
        );
        results.push(result);
    }

    let report = RunReport::new(
        manifest.suite,
        manifest.upstream,
        &cli.browser,
        &cli.manifest,
        cli.settle_ms,
        cli.timeout_ms,
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
    println!("JSON report: {}", cli.output.display());
    Ok(report.summary.is_success())
}
