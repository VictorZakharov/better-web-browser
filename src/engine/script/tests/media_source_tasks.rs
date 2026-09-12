use super::*;

/// Media fixtures inspect commands and DOM state after an acknowledgement and its queued
/// event tasks. Do not make production input dispatch execute future tasks reentrantly.
pub(in crate::engine::script::tests) trait MediaTaskTestRuntime {
    fn dispatch_media_and_tasks(&mut self, event: UserInputEvent) -> UserInputResult;
}

impl MediaTaskTestRuntime for ScriptRuntime {
    fn dispatch_media_and_tasks(&mut self, event: UserInputEvent) -> UserInputResult {
        assert!(matches!(event, UserInputEvent::Media { .. }));
        let mut result = self.dispatch_user_input(event);
        for _ in 0..128 {
            if self.next_timer_delay() != Some(Duration::ZERO) {
                return result;
            }
            let mut task = self.advance_time(Duration::ZERO, 1);
            assert!(!task.runtime_stopped, "{:?}", task.errors);
            result.outcome.errors.append(&mut task.errors);
            result.outcome.media_actions.append(&mut task.media_actions);
        }
        panic!("media fixture did not settle within 128 tasks");
    }
}

#[test]
fn media_source_open_is_a_task_after_the_current_microtask_checkpoint() {
    let (dom, _, outcome) = execute_media_source(
        r#"<body><video></video><output></output><script>
            const source = new MediaSource();
            const output = document.querySelector('output');
            source.addEventListener('sourceopen', () => output.textContent += '|open');
            document.querySelector('video').src = URL.createObjectURL(source);
            Promise.resolve().then(() => output.textContent += '|microtask');
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "|microtask|open"
    );
}

#[test]
fn source_buffer_events_have_separate_microtask_checkpoints() {
    let (dom, _, outcome) = execute_media_source(
        r#"<body><video></video><output></output><script>
            const source = new MediaSource();
            const log = [];
            source.addEventListener('sourceopen', () => {
                const buffer = source.addSourceBuffer('video/mp4; codecs="avc1.4d401e"');
                for (const name of ['updatestart','update','updateend']) {
                    buffer.addEventListener(name, () => {
                        log.push(name);
                        queueMicrotask(() => {
                            log.push('micro-' + name);
                            document.querySelector('output').textContent = log.join('|');
                        });
                    });
                }
                buffer.appendBuffer(new Uint8Array());
                Promise.resolve().then(() => log.push('after-append'));
            });
            document.querySelector('video').src = URL.createObjectURL(source);
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "after-append|updatestart|micro-updatestart|update|micro-update|updateend|micro-updateend"
    );
}

#[test]
fn media_tasks_are_not_cancelable_or_replaceable_with_author_timer_apis() {
    let (dom, _, outcome) = execute_media_source(
        r#"<video></video><output></output><script>
            window.setTimeout = window.queueMicrotask = () => { throw Error('author API used'); };
            const source = new MediaSource();
            source.addEventListener('sourceopen', event => {
                document.querySelector('output').textContent = String(event.isTrusted);
            });
            document.querySelector('video').src = URL.createObjectURL(source);
            for (let id = 1; id < 100; id++) { clearTimeout(id); clearInterval(id); }
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "true"
    );
}

#[test]
fn microtask_after_updatestart_can_abort_before_the_parser_task() {
    let (dom, _, outcome) = execute_media_source(
        r#"<video></video><output></output><script>
            const source = new MediaSource(), log = [];
            source.addEventListener('sourceopen', () => {
                const buffer = source.addSourceBuffer('video/mp4; codecs="avc1.4d401e"');
                buffer.addEventListener('updatestart', () => {
                    log.push('start');
                    Promise.resolve().then(() => { log.push('microtask'); buffer.abort(); });
                });
                for (const event of ['abort', 'error', 'update', 'updateend']) {
                    buffer.addEventListener(event, () => {
                        log.push(event);
                        document.querySelector('output').textContent = log.join('|');
                    });
                }
                buffer.appendBuffer(new Uint8Array([0,0,0,1,109,100,97,116,255,255,255,255,255,255,255,255]));
            });
            document.querySelector('video').src = URL.createObjectURL(source);
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert!(outcome.media_actions.iter().all(|action| !matches!(
        action.command,
        ScriptMediaCommand::Commit { .. } | ScriptMediaCommand::CommitAdaptive { .. }
    )));
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "start|microtask|abort|updateend"
    );
}
