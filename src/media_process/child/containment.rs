use super::*;
pub(super) fn containment_report() -> Result<ContainmentReport, String> {
    Ok(ContainmentReport {
        app_container: is_app_container()?,
        no_console_window: unsafe { GetConsoleWindow() }.is_null(),
        minimal_environment: has_minimal_environment(),
    })
}

pub(super) fn has_minimal_environment() -> bool {
    let mut system_root = false;
    for (name, _) in std::env::vars_os() {
        let name = name.to_string_lossy();
        if name.eq_ignore_ascii_case("SystemRoot") {
            system_root = true;
        }
        if !crate::renderer_process::renderer_environment_name_allowed(&name) {
            return false;
        }
    }
    system_root
}

pub(super) fn is_app_container() -> Result<bool, String> {
    let mut token = null_mut();
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return Err("open media process token".into());
    }
    let mut app_container = 0_u32;
    let mut returned = 0_u32;
    let ok = unsafe {
        GetTokenInformation(
            token,
            TokenIsAppContainer,
            (&mut app_container as *mut u32).cast(),
            size_of::<u32>() as u32,
            &mut returned,
        )
    };
    unsafe { windows_sys::Win32::Foundation::CloseHandle(token) };
    (ok != 0 && returned as usize == size_of::<u32>())
        .then_some(app_container != 0)
        .ok_or_else(|| "query media AppContainer token".into())
}

pub(super) fn probe_restrictions(loopback_port: u16) -> MediaRestrictionReport {
    let child_error = probe_child_launch();
    let loopback_error = probe_network(SocketAddr::from(([127, 0, 0, 1], loopback_port)));
    let internet_error = probe_network(SocketAddr::from(([1, 1, 1, 1], 443)));
    MediaRestrictionReport {
        child_launch_denied: is_access_denied(child_error),
        loopback_denied: loopback_error != 0,
        internet_denied: is_access_denied(internet_error),
        child_error,
        loopback_error,
        internet_error,
    }
}

pub(super) fn probe_child_launch() -> i32 {
    let executable = match std::env::current_exe() {
        Ok(path) => path,
        Err(error) => return error.raw_os_error().unwrap_or(-1),
    };
    match Command::new(executable)
        .arg("--media-child-probe")
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
    {
        Ok(mut child) => {
            let _ = child.kill();
            let _ = child.wait();
            0
        }
        Err(error) => error.raw_os_error().unwrap_or(-1),
    }
}

pub(super) fn probe_network(address: SocketAddr) -> i32 {
    // The diagnostic must remain comfortably inside the broker's bounded command deadline even
    // when Windows reports an isolated route by timing out rather than returning access denied.
    match TcpStream::connect_timeout(&address, Duration::from_millis(200)) {
        Ok(_) => 0,
        Err(error) => error.raw_os_error().unwrap_or_else(|| match error.kind() {
            std::io::ErrorKind::PermissionDenied => -2,
            std::io::ErrorKind::TimedOut => -3,
            _ => -1,
        }),
    }
}

pub(super) fn is_access_denied(error: i32) -> bool {
    matches!(error, 5 | 10_013)
}
