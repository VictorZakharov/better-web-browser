use super::*;

fn link(id: u32, shaders: &[u32]) -> Link {
    Link {
        id,
        native: id + 100,
        shaders: shaders
            .iter()
            .map(|&id| Shader {
                id,
                native: id + 100,
            })
            .collect(),
    }
}

#[test]
fn submission_retains_invocation_order_and_native_names() {
    let mut queue = PendingLinks::default();
    let first = link(1, &[11, 12]);
    let second = link(2, &[21, 22]);
    queue.push(first.clone()).unwrap();
    queue.push(second.clone()).unwrap();
    assert_eq!(queue.front(), Some(&first));
    assert_eq!(queue.pop(), Some(first));
    assert_eq!(queue.pop(), Some(second));
    assert!(queue.is_empty());
    assert_eq!(queue.pop(), None);
}

#[test]
fn program_barrier_includes_earlier_unrelated_invocations() {
    let mut queue = PendingLinks::default();
    for id in 1..=3 {
        queue.push(link(id, &[])).unwrap();
    }
    assert_eq!(queue.through_program(2), 2);
    assert_eq!(queue.through_program(99), 0);
    queue.pop();
    assert_eq!(queue.through_program(2), 1);
}

#[test]
fn shared_shader_mutation_resolves_every_prior_snapshot() {
    let mut queue = PendingLinks::default();
    queue.push(link(1, &[10, 11])).unwrap();
    queue.push(link(2, &[20, 21])).unwrap();
    queue.push(link(3, &[10, 31])).unwrap();
    queue.push(link(4, &[40, 41])).unwrap();
    assert_eq!(queue.through_shader(10), 3);
    assert_eq!(queue.through_shader(20), 2);
    assert_eq!(queue.through_shader(999), 0);
}

#[test]
fn capacity_is_fixed_and_rejection_keeps_existing_jobs() {
    let mut queue = PendingLinks::default();
    for id in 1..=LIMIT as u32 {
        queue.push(link(id, &[1000, 1001])).unwrap();
    }
    let overflow = link(LIMIT as u32 + 1, &[1000, 1001]);
    assert_eq!(queue.push(overflow.clone()), Err(overflow.clone()));
    assert_eq!(queue.len(), LIMIT);
    assert_eq!(queue.pop().unwrap().id, 1);
    queue.push(overflow).unwrap();
    assert_eq!(queue.len(), LIMIT);
}

#[test]
fn relink_requires_an_ordering_barrier_not_replacement() {
    let mut queue = PendingLinks::default();
    let original = link(1, &[10, 11]);
    queue.push(original.clone()).unwrap();
    let replacement = link(1, &[20, 21]);
    assert_eq!(queue.push(replacement.clone()), Err(replacement.clone()));
    assert_eq!(queue.pop(), Some(original));
    queue.push(replacement.clone()).unwrap();
    assert_eq!(queue.pop(), Some(replacement));
}

#[test]
fn snapshots_are_bounded_to_webgl_shader_stages() {
    let mut queue = PendingLinks::default();
    let invalid = link(1, &[10, 11, 12]);
    assert_eq!(queue.push(invalid.clone()), Err(invalid));
    assert!(queue.is_empty());
    queue.push(link(1, &[])).unwrap();
}
