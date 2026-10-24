use super::*;

fn event(id: u32) -> Event {
    Event {
        id,
        native: id + 100,
    }
}

#[test]
fn shader_and_program_staging_are_independently_bounded() {
    let mut events = Events::default();
    for shader in [true, false] {
        let limit = if shader { SHADERS } else { PROGRAMS };
        for id in 1..=limit as u32 {
            events.record(shader, event(id)).unwrap();
        }
        let overflow = event(limit as u32 + 1);
        assert_eq!(events.record(shader, overflow), Err(overflow));
        assert_eq!(events.len(shader), limit);
    }
    assert_eq!(events.len(true), SHADERS);
    assert_eq!(events.len(false), PROGRAMS);
}

#[test]
fn an_invocation_is_recorded_once_and_recompile_can_replace_it() {
    let mut events = Events::default();
    events.record(true, event(1)).unwrap();
    assert_eq!(events.record(true, event(1)), Err(event(1)));
    events.remove(true, 1);
    let replacement = Event { id: 1, native: 999 };
    events.record(true, replacement).unwrap();
    assert_eq!(events.pop(true), Some(replacement));
    assert_eq!(events.pop(true), None);
}

#[test]
fn rotating_unready_work_visits_other_events_without_losing_ownership() {
    let mut events = Events::default();
    for id in 1..=3 {
        events.record(false, event(id)).unwrap();
    }
    let pending = events.pop(false).unwrap();
    events.record(false, pending).unwrap();
    assert_eq!(events.pop(false), Some(event(2)));
    assert_eq!(events.pop(false), Some(event(3)));
    assert_eq!(events.pop(false), Some(event(1)));
    assert_eq!(events.len(false), 0);
}

#[test]
fn removal_never_affects_peer_jobs_or_the_other_stage() {
    let mut events = Events::default();
    for shader in [true, false] {
        events.record(shader, event(1)).unwrap();
        events.record(shader, event(2)).unwrap();
    }
    events.remove(true, 1);
    events.remove(false, 99);
    assert_eq!(events.len(true), 1);
    assert_eq!(events.len(false), 2);
    assert_eq!(events.pop(true), Some(event(2)));
    assert_eq!(events.pop(false), Some(event(1)));
}
