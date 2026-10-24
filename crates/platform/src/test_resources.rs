//! Fail before native GPU setup if a CPU-only test process was misclassified.
//! The complete verification runner owns this environment flag per child.
//! Native device setup calls this in every build, not only under `cfg(test)`,
//! so dependent crates' unit tests are guarded too; without the flag it only
//! reads one environment variable.
pub fn require_native_gpu() {
    if std::env::var_os("BREEZE_TEST_CPU_ONLY").is_some() {
        // A recoverable error could satisfy a negative test and hide a bad map.
        // Exit the owned test process so the complete gate necessarily fails.
        // Bypass libtest capture: exit does not flush its per-test stderr buffer.
        use std::io::Write;
        let _ = writeln!(
            std::io::stderr().lock(),
            "CPU test worker attempted native GPU setup; move this test module to the native worker in scripts/full-test-plan.ps1"
        );
        std::process::exit(86);
    }
}

#[test]
fn cpu_resource_guard_is_fail_closed() {
    use std::process::{Command, Stdio};
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "test_resources::cpu_resource_guard_child",
            "--ignored",
        ])
        .env("BREEZE_TEST_CPU_ONLY", "1")
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let result = command.output().unwrap();
    assert_eq!(result.status.code(), Some(86));
    assert!(
        String::from_utf8_lossy(&result.stderr)
            .contains("CPU test worker attempted native GPU setup")
    );
}

#[test]
#[ignore = "subprocess probe for the CPU resource guard"]
fn cpu_resource_guard_child() {
    if std::env::var_os("BREEZE_TEST_CPU_ONLY").is_some() {
        require_native_gpu();
        panic!("CPU guard returned instead of failing the worker");
    }
}
