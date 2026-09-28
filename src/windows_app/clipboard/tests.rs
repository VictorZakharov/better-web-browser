use super::*;
use std::cell::{Cell, RefCell};
use std::ptr::null_mut;
use std::rc::Rc;

#[derive(Clone, Default)]
struct FakeClipboard {
    text: Rc<RefCell<Option<String>>>,
    reads: Rc<Cell<usize>>,
    writes: Rc<Cell<usize>>,
}

impl ClipboardBackend for FakeClipboard {
    fn read_text(&self, _owner: Hwnd) -> Result<String, ClipboardError> {
        self.reads.set(self.reads.get() + 1);
        self.text.borrow().clone().ok_or(ClipboardError::NotFound)
    }

    fn write_text(&self, _owner: Hwnd, text: &str) -> Result<(), ClipboardError> {
        self.writes.set(self.writes.get() + 1);
        *self.text.borrow_mut() = Some(text.into());
        Ok(())
    }
}

#[test]
fn session_grants_are_separate_by_origin_and_operation() {
    let fake = FakeClipboard::default();
    let mut service = ClipboardService::with_backend(Box::new(fake.clone()));
    let a = Origin::parse("https://a.example/page").unwrap();
    let b = Origin::parse("https://b.example/page").unwrap();
    assert_eq!(service.decision(&a, Access::Read), None);
    service.decide(a.clone(), Access::Read, true);
    service.decide(a.clone(), Access::Write, false);
    assert_eq!(service.decision(&a, Access::Read), Some(true));
    assert_eq!(service.decision(&a, Access::Write), Some(false));
    assert_eq!(service.decision(&b, Access::Read), None);
    assert_eq!(service.read_text(null_mut()), Err(ClipboardError::NotFound));
    assert_eq!(fake.reads.get(), 1);
    service.write_text(null_mut(), "real text").unwrap();
    assert_eq!(service.read_text(null_mut()).unwrap(), "real text");
    assert_eq!(fake.writes.get(), 1);
}

#[test]
fn grants_do_not_survive_service_replacement() {
    let origin = Origin::parse("https://example.org/").unwrap();
    let mut service = ClipboardService::with_backend(Box::new(FakeClipboard::default()));
    service.decide(origin.clone(), Access::Read, true);
    assert_eq!(service.decision(&origin, Access::Read), Some(true));
    let replacement = ClipboardService::with_backend(Box::new(FakeClipboard::default()));
    assert_eq!(replacement.decision(&origin, Access::Read), None);
}
