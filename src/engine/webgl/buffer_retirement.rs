//! Keep native numeric identity unique until retained containers release storage.
use super::{MAX_OBJECTS, Result, gl};

pub(super) fn reserve_deleted_name(retired: u32) -> Result<()> {
    // The pinned ANGLE State::detachBuffer compares numeric IDs, not allocation
    // identities. Reusing an ID while an inactive VAO retains its old allocation
    // lets deletion of the new allocation detach the old one. GenBuffers reserves
    // a name without instantiating a Buffer; deleting that reservation later does
    // not call State::detachBuffer. Never bind this private reservation.
    // ANGLE allocates released names before its unallocated range. Other holes
    // may precede `retired`, so hold them until the wanted name is reserved.
    // Browser buffers are the only native buffer allocator, bounded by MAX_OBJECTS
    // plus the single allocation checked before public-name insertion.
    let mut skipped = Vec::new();
    let mut found = false;
    for _ in 0..=MAX_OBJECTS {
        let mut name = 0;
        unsafe {
            gl::GenBuffers(1, &mut name);
        }
        if name == retired {
            found = true;
            break;
        }
        if name == 0 {
            break;
        }
        skipped.push(name);
    }
    unsafe {
        gl::DeleteBuffers(skipped.len() as i32, skipped.as_ptr());
    }
    if found {
        Ok(())
    } else {
        Err(gl::OUT_OF_MEMORY)
    }
}
