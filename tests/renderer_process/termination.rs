use super::support::*;
use better_web_browser::renderer_process::{RendererEvent, RendererExitReason, RendererSession};
use better_web_browser::renderer_protocol::{
    DocumentInput, DocumentNodeId, TestCommand, TextInput,
};
use std::ffi::c_void;
use std::time::Duration;

#[test]
fn browser_owned_termination_stops_the_real_renderer_and_allows_replacement() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let session = RendererSession::launch(options()).expect("launch renderer");
    let original = session.snapshot();
    load_inline_document(&session, 120);
    session.terminate().expect("request renderer termination");
    let exit = session
        .wait_for_exit(Duration::from_secs(3))
        .expect("renderer termination exit");
    assert_eq!(exit.reason, RendererExitReason::Terminated);
    assert!(
        exit.crash_surface()
            .is_some_and(|surface| surface.can_reload)
    );
    drop(session);

    let mut replacement = RendererSession::launch(options()).expect("launch replacement renderer");
    assert_ne!(replacement.snapshot().process_id, original.process_id);
    load_inline_document(&replacement, 121);
    replacement
        .shutdown()
        .expect("shutdown replacement renderer");
}

#[test]
fn browser_owned_background_termination_bypasses_a_saturated_page_command_queue() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut launch = options();
    launch.unresponsive_timeout = Duration::from_secs(10);
    let session = RendererSession::launch(launch).expect("launch renderer");
    let process = unsafe { OpenProcess(SYNCHRONIZE, 0, session.snapshot().process_id) };
    assert!(!process.is_null(), "open renderer process wait handle");
    let presentation = load_inline_document(&session, 122);
    session
        .send_test_command(TestCommand::Hang)
        .expect("hang renderer child");

    let target = DocumentNodeId::new((1_u128 << 64) | 1).unwrap();
    let value = "x".repeat(60 * 1024);
    let mut saturated = false;
    for sequence in 1..=256 {
        match session.send_input(DocumentInput::Text(TextInput {
            document: presentation.document,
            sequence,
            target,
            value: value.clone(),
            selection_start: value.len() as u32,
            selection_end: value.len() as u32,
        })) {
            Ok(()) => {}
            Err(error) if error.contains("command queue is full") => {
                saturated = true;
                break;
            }
            Err(error) => panic!("unexpected input enqueue failure: {error}"),
        }
    }
    assert!(
        saturated,
        "hung renderer did not apply command backpressure"
    );

    session.terminate_in_background();
    let wait = unsafe { WaitForSingleObject(process, 3_000) };
    unsafe { CloseHandle(process) };
    assert_eq!(
        wait, WAIT_OBJECT_0,
        "background termination was not immediate"
    );
}

#[test]
fn watchdog_survives_a_blocked_renderer_command_pipe() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let session = RendererSession::launch(hung_task_options()).expect("launch renderer");
    let presentation = load_inline_document(&session, 123);
    session
        .send_test_command(TestCommand::Hang)
        .expect("hang renderer child");

    let target = DocumentNodeId::new((1_u128 << 64) | 1).unwrap();
    let value = "x".repeat(60 * 1024);
    let mut saturated = false;
    for sequence in 1..=256 {
        let result = session.send_input(DocumentInput::Text(TextInput {
            document: presentation.document,
            sequence,
            target,
            value: value.clone(),
            selection_start: value.len() as u32,
            selection_end: value.len() as u32,
        }));
        if result
            .as_ref()
            .is_err_and(|error| error.contains("command queue is full"))
        {
            saturated = true;
            break;
        }
        result.expect("enqueue renderer pressure");
    }
    assert!(
        saturated,
        "blocked renderer did not saturate its command path"
    );

    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    loop {
        assert!(
            std::time::Instant::now() < deadline,
            "broker watchdog was blocked behind the renderer command pipe"
        );
        if let Ok(RendererEvent::Unresponsive) = session.wait_for_event(Duration::from_millis(500))
        {
            break;
        }
    }
    let exit = session
        .wait_for_exit(Duration::from_secs(3))
        .expect("watchdog terminated the blocked renderer");
    assert!(matches!(
        exit.reason,
        RendererExitReason::TaskBudgetExceeded(_)
    ));
}

const SYNCHRONIZE: u32 = 0x0010_0000;
const WAIT_OBJECT_0: u32 = 0;

#[link(name = "kernel32")]
unsafe extern "system" {
    fn OpenProcess(access: u32, inherit: i32, process_id: u32) -> *mut c_void;
    fn WaitForSingleObject(handle: *mut c_void, milliseconds: u32) -> u32;
    fn CloseHandle(handle: *mut c_void) -> i32;
}
