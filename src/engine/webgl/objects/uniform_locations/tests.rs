use super::*;

fn record(owner: u32, generation: u32, native: u32) -> UniformLocation {
    UniformLocation {
        owner,
        generation,
        native,
        uniform_type: gl::FLOAT,
    }
}

#[test]
fn uniform_metadata_zero_location_and_aliases_keep_typed_identity() {
    let mut locations = UniformLocations::default();
    let id = locations.insert(record(1, 2, 0)).unwrap();
    assert_ne!(id, 0);
    assert_eq!(locations.insert(record(1, 2, 0)), Ok(id));
    assert_ne!(locations.insert(record(2, 2, 0)), Ok(id));
    assert_ne!(locations.insert(record(1, 3, 0)), Ok(id));
    assert_eq!(locations.find(1, 2, 0), Some(id));
    assert_eq!(locations.get(id), Ok(&record(1, 2, 0)));
    assert_eq!(locations.get(0), Err(gl::INVALID_OPERATION));
}

#[test]
fn uniform_metadata_cap_is_atomic_and_existing_aliases_still_work() {
    let mut locations = UniformLocations::default();
    let first = locations.insert(record(1, 0, 0)).unwrap();
    for native in 1..MAX_LOCATIONS as u32 {
        locations.insert(record(1, 0, native)).unwrap();
    }
    assert_eq!(locations.insert(record(2, 0, 0)), Err(gl::OUT_OF_MEMORY));
    assert_eq!(locations.insert(record(1, 0, 0)), Ok(first));
    assert_eq!(locations.records.len(), MAX_LOCATIONS);
    assert_eq!(locations.names.len(), MAX_LOCATIONS);
    locations.retire(1);
    assert!(locations.records.is_empty());
    assert!(locations.names.is_empty());
    let next = locations.insert(record(2, 0, 0)).unwrap();
    assert_ne!(next, first);
    assert_eq!(locations.get(first), Err(gl::INVALID_OPERATION));
}

#[test]
fn uniform_metadata_retirement_is_owner_scoped_and_names_never_recycle() {
    let mut locations = UniformLocations::default();
    let old = locations.insert(record(1, 0, 7)).unwrap();
    let peer = locations.insert(record(2, 0, 7)).unwrap();
    locations.retire(1);
    assert_eq!(locations.get(old), Err(gl::INVALID_OPERATION));
    assert_eq!(locations.get(peer), Ok(&record(2, 0, 7)));
    assert_eq!(locations.find(1, 0, 7), None);
    let fresh = locations.insert(record(1, 1, 7)).unwrap();
    assert_ne!(fresh, old);
    locations.clear();
    assert_eq!(locations.get(peer), Err(gl::INVALID_OPERATION));
    assert!(locations.names.is_empty());
}

#[test]
fn uniform_metadata_does_not_consume_gpu_object_capacity() {
    let mut objects = super::super::Objects::default();
    let program = objects.insert(super::super::Kind::Program, 1).unwrap();
    for native in 0..MAX_LOCATIONS as u32 {
        objects.insert_uniform(program, native, gl::FLOAT).unwrap();
    }
    for native in 2..=crate::engine::webgl::MAX_OBJECTS as u32 {
        objects.insert(super::super::Kind::Shader, native).unwrap();
    }
    assert_eq!(
        objects.insert(super::super::Kind::Shader, 2000),
        Err(gl::OUT_OF_MEMORY)
    );
    assert_eq!(objects.begin_link(program), Ok(1));
    assert_eq!(objects.uniform(program, 0, 0), None);
    assert!(objects.insert_uniform(program, 0, gl::FLOAT).is_ok());
}
