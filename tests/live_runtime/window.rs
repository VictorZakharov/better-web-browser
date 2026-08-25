use std::ffi::c_void;
use std::thread;
use std::time::{Duration, Instant};

pub(super) fn reload_when_title_contains(
    child: &std::process::Child,
    expected: &str,
    timeout: Duration,
) {
    let window = wait_for_title(child.id(), expected, timeout);
    let posted = unsafe { PostMessageW(window as *mut c_void, WM_COMMAND, RELOAD_COMMAND_ID, 0) };
    assert_ne!(posted, 0, "post reload command to hidden browser");
}

fn wait_for_title(process_id: u32, expected: &str, timeout: Duration) -> usize {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(window) = browser_window(process_id) {
            let mut title = [0_u16; 512];
            let length = unsafe {
                GetWindowTextW(
                    window as *mut c_void,
                    title.as_mut_ptr(),
                    title.len() as i32,
                )
            };
            let title = String::from_utf16_lossy(&title[..length.max(0) as usize]);
            if title.contains(expected) {
                return window;
            }
        }
        assert!(
            Instant::now() < deadline,
            "hidden browser did not reach title {expected:?} before reload"
        );
        thread::sleep(Duration::from_millis(25));
    }
}

fn browser_window(process_id: u32) -> Option<usize> {
    let mut search = WindowSearch {
        process_id,
        window: 0,
    };
    unsafe {
        EnumWindows(
            Some(find_browser_window),
            (&mut search as *mut WindowSearch) as isize,
        );
    }
    (search.window != 0).then_some(search.window)
}

unsafe extern "system" fn find_browser_window(window: *mut c_void, context: isize) -> i32 {
    let search = unsafe { &mut *(context as *mut WindowSearch) };
    let mut process_id = 0_u32;
    unsafe {
        GetWindowThreadProcessId(window, &mut process_id);
    }
    if process_id == search.process_id {
        search.window = window as usize;
        0
    } else {
        1
    }
}

struct WindowSearch {
    process_id: u32,
    window: usize,
}

const WM_COMMAND: u32 = 0x0111;
const RELOAD_COMMAND_ID: usize = 1003;

#[link(name = "user32")]
unsafe extern "system" {
    fn EnumWindows(
        callback: Option<unsafe extern "system" fn(*mut c_void, isize) -> i32>,
        context: isize,
    ) -> i32;
    fn GetWindowThreadProcessId(window: *mut c_void, process_id: *mut u32) -> u32;
    fn GetWindowTextW(window: *mut c_void, text: *mut u16, maximum: i32) -> i32;
    fn PostMessageW(window: *mut c_void, message: u32, wparam: usize, lparam: isize) -> i32;
}
