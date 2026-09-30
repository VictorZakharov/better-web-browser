use super::*;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;

#[derive(Debug, PartialEq, Eq)]
enum Message {
    Input(u32),
    Posted(u32),
}

#[test]
fn continuous_input_preserves_fifo_and_gives_posted_work_a_finite_turn() {
    let inputs = RefCell::new((0..25).map(Message::Input).collect::<VecDeque<_>>());
    let mut posted = (0..3).map(Message::Posted).collect::<VecDeque<_>>();
    let mut policy = InputFirstPolicy::default();
    let mut delivered = Vec::new();
    while let Some(message) = policy
        .next(
            || inputs.borrow_mut().pop_front(),
            || {
                Ok::<_, ()>(
                    posted
                        .pop_front()
                        .or_else(|| inputs.borrow_mut().pop_front()),
                )
            },
            || panic!("no translation has armed a character check"),
        )
        .unwrap()
    {
        delivered.push(message);
    }
    assert_eq!(delivered.len(), 28, "every original message is delivered");
    assert_eq!(delivered[8], Message::Posted(0));
    assert_eq!(delivered[17], Message::Posted(1));
    assert_eq!(delivered[26], Message::Posted(2));
    assert_eq!(
        delivered
            .into_iter()
            .filter_map(|message| match message {
                Message::Input(sequence) => Some(sequence),
                Message::Posted(_) => None,
            })
            .collect::<Vec<_>>(),
        (0..25).collect::<Vec<_>>()
    );
}

#[test]
fn empty_input_falls_back_once_without_spinning_and_resets_the_burst() {
    let peeks = Cell::new(0);
    let reads = Cell::new(0);
    let mut policy = InputFirstPolicy {
        preferred_reads: 7,
        ..Default::default()
    };
    let result = policy.next(
        || {
            peeks.set(peeks.get() + 1);
            None::<u32>
        },
        || {
            reads.set(reads.get() + 1);
            Ok::<_, ()>(None)
        },
        || panic!("no translation has armed a character check"),
    );
    assert_eq!(result, Ok(None));
    assert_eq!(peeks.get(), 1);
    assert_eq!(reads.get(), 1);
    assert_eq!(policy.preferred_reads, 0);
}

#[test]
fn ordinary_quit_and_error_propagate_after_the_input_budget() {
    for fail in [false, true] {
        let mut policy = InputFirstPolicy::default();
        for sequence in 0..MAX_PREFERRED_INPUTS {
            assert_eq!(
                policy.next(
                    || Some(sequence),
                    || Err("unexpected ordinary read"),
                    || panic!("no translation has armed a character check"),
                ),
                Ok(Some(sequence))
            );
        }
        let result: Result<Option<u8>, &str> = policy.next(
            || panic!("an exhausted priority budget must not peek again"),
            || {
                if fail {
                    Err("native failure")
                } else {
                    Ok(None)
                }
            },
            || panic!("no translation has armed a character check"),
        );
        assert_eq!(
            result,
            if fail {
                Err("native failure")
            } else {
                Ok(None)
            }
        );
        assert_eq!(policy.preferred_reads, 0);
    }
}

#[test]
fn input_selection_uses_the_sdk_queue_source_mask_not_message_numbers() {
    assert_eq!(PM_REMOVE | PM_QS_INPUT, 0x1c07_0001);
    assert_eq!(PM_QS_POSTED_CHARACTERS, 0x0008_0000, "PM_NOREMOVE is zero");
    assert_eq!(
        POSTED_CHARACTER_MESSAGES,
        [0x102, 0x103, 0x106, 0x107, 0x109]
    );
}

#[test]
fn translation_without_a_posted_character_resumes_preferred_input_immediately() {
    let mut policy = InputFirstPolicy::default();
    policy.note_translation(true);
    assert_eq!(
        policy.next(
            || Some("keyup"),
            || Err("ordinary retrieval is unnecessary without a character"),
            || false,
        ),
        Ok(Some("keyup")),
    );
    assert!(!policy.translated_input_pending);
    assert_eq!(policy.preferred_reads, 1);
}

#[test]
fn every_translated_character_drains_in_posted_fifo_before_keys_and_focus_changes() {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum KeyboardMessage {
        KeyDown,
        KeyUp,
        FocusChange,
        PostedWork(u8),
        Character(u32, u16),
    }
    for kind in POSTED_CHARACTER_MESSAGES {
        use KeyboardMessage::*;
        let hardware = RefCell::new(VecDeque::from([KeyDown, KeyUp, FocusChange]));
        let posted = RefCell::new(VecDeque::new());
        let mut policy = InputFirstPolicy::default();
        let mut delivered = Vec::new();
        while let Some(message) = policy
            .next(
                || hardware.borrow_mut().pop_front(),
                || {
                    Ok::<_, ()>(
                        posted
                            .borrow_mut()
                            .pop_front()
                            .or_else(|| hardware.borrow_mut().pop_front()),
                    )
                },
                || {
                    posted
                        .borrow()
                        .iter()
                        .any(|message| matches!(message, Character(_, _)))
                },
            )
            .unwrap()
        {
            delivered.push(message);
            if message == KeyDown {
                posted.borrow_mut().extend([
                    PostedWork(1),
                    Character(kind, 0xd83d),
                    PostedWork(2),
                    Character(kind, 0xde00),
                ]);
                policy.note_translation(true);
            } else {
                // Character/posted dispatch returning zero cannot disarm the
                // second code unit; keyup's nonzero may produce no character.
                policy.note_translation(message == KeyUp);
            }
        }
        assert_eq!(
            delivered,
            [
                KeyDown,
                PostedWork(1),
                Character(kind, 0xd83d),
                PostedWork(2),
                Character(kind, 0xde00),
                KeyUp,
                FocusChange,
            ]
        );
        assert!(!policy.translated_input_pending);
    }
}

#[test]
fn pending_character_guard_preserves_ordinary_quit_and_error_handling() {
    for fail in [false, true] {
        let mut policy = InputFirstPolicy::default();
        policy.note_translation(true);
        let result: Result<Option<u8>, &str> = policy.next(
            || panic!("hardware cannot overtake a translated character"),
            || {
                if fail {
                    Err("native failure")
                } else {
                    Ok(None)
                }
            },
            || true,
        );
        assert_eq!(
            result,
            if fail {
                Err("native failure")
            } else {
                Ok(None)
            }
        );
    }
}
