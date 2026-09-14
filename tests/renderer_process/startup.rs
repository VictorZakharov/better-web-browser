use super::{SERIAL, assert_handle_count_returns_to, options, process_handle_count};
use better_web_browser::renderer_process::{RendererSession, StartupFault};
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[test]
fn startup_faults_fail_closed_within_the_deadline() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    // GetProcessHandleCount includes libtest's concurrently created worker threads and
    // their waits. Run the process-wide leak assertion in a dedicated test process.
    if std::env::var_os("BREEZE_STARTUP_HANDLE_CHECK_CHILD").as_deref()
        != Some(std::ffi::OsStr::new("1"))
    {
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "startup::startup_faults_fail_closed_within_the_deadline",
                "--test-threads=1",
                "--nocapture",
            ])
            .env("BREEZE_STARTUP_HANDLE_CHECK_CHILD", "1")
            .creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(30);
        while child.try_wait().unwrap().is_none() {
            if Instant::now() >= deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("isolated startup cleanup check exceeded its deadline");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let mut warmup = RendererSession::launch(options()).expect("warm renderer infrastructure");
    warmup.shutdown().expect("shutdown warmup renderer");
    drop(warmup);
    // Successful startup alone does not warm the OS failure/wait path. On Windows,
    // the first timed-out startup retained one WaitCompletionPacket; 15 subsequent
    // failed starts retained no additional handles. Warm both paths before the
    // steady-state leak check, rather than allowing a growing handle-count margin.
    exercise_faults();
    let before = process_handle_count();
    for _ in 0..3 {
        exercise_faults();
        assert_handle_count_returns_to(before);
    }
}

fn exercise_faults() {
    for fault in [
        StartupFault::Silent,
        StartupFault::WrongNonce,
        StartupFault::MalformedFrame,
        StartupFault::OversizedFrame,
        StartupFault::IncompatibleVersion,
    ] {
        let mut launch = options();
        launch.startup_timeout = Duration::from_millis(200);
        launch.startup_fault = Some(fault);
        let started = Instant::now();
        let result = RendererSession::launch(launch);
        assert!(result.is_err(), "startup fault was accepted: {fault:?}");
        assert!(started.elapsed() < Duration::from_secs(3));
    }
}
