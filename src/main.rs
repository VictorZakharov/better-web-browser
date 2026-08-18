#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

#[cfg(target_os = "windows")]
mod windows_app;

#[cfg(target_os = "windows")]
fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if let Some(result) = better_web_browser::renderer_process::run_child_from_args(&arguments) {
        if result.is_err() {
            std::process::exit(70);
        }
        return;
    }
    if let Err(error) = require_hidden_benchmark_when_automated(
        &arguments,
        std::env::var_os("BREEZE_REQUIRE_HIDDEN_BENCHMARK").is_some(),
    ) {
        eprintln!("{error}");
        std::process::exit(64);
    }
    let benchmark = arguments.iter().any(|argument| argument == "--benchmark");
    let large_headless_stack =
        benchmark && std::env::var_os("BREEZE_HEADLESS_LARGE_STACK").is_some();
    let result = if large_headless_stack {
        run_benchmark()
    } else {
        windows_app::run()
    };
    if let Err(error) = result {
        if benchmark {
            eprintln!("{error}");
            std::process::exit(1);
        } else {
            windows_app::show_fatal_error(&error);
        }
    }
}

#[cfg(target_os = "windows")]
fn require_hidden_benchmark_when_automated(
    arguments: &[String],
    automation_guard_enabled: bool,
) -> Result<(), &'static str> {
    if automation_guard_enabled && !arguments.iter().any(|argument| argument == "--benchmark") {
        return Err("refusing an interactive launch: guarded automation requires --benchmark");
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn run_benchmark() -> Result<(), String> {
    // The WPT runner opts into this path because large standards harnesses create deeper debug-
    // build call stacks than ordinary pages. Other benchmark consumers keep the normal main-thread
    // path so performance measurements do not gain a helper thread.
    std::thread::Builder::new()
        .name("breeze-headless".to_string())
        .stack_size(8 * 1024 * 1024)
        .spawn(windows_app::run)
        .map_err(|error| format!("start hidden browser thread: {error}"))?
        .join()
        .map_err(|_| "hidden browser thread panicked".to_string())?
}

#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!("This MVP currently uses the Windows-native fast path. A portable shell is planned.");
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::require_hidden_benchmark_when_automated;

    #[test]
    fn guarded_automation_rejects_an_interactive_launch() {
        let error = require_hidden_benchmark_when_automated(&[], true).unwrap_err();

        assert!(error.contains("requires --benchmark"));
    }

    #[test]
    fn guarded_automation_accepts_hidden_benchmark_arguments() {
        let arguments = vec![
            "--benchmark".to_string(),
            "https://example.test/".to_string(),
        ];

        assert!(require_hidden_benchmark_when_automated(&arguments, true).is_ok());
    }

    #[test]
    fn ordinary_interactive_launches_remain_available() {
        assert!(require_hidden_benchmark_when_automated(&[], false).is_ok());
    }
}
