use super::*;
use crate::storage::{StorageError, StorageString};

#[test]
fn storage_write_queue_is_bounded_but_noops_and_cleanup_still_work() {
    let dom = dom::parse_with_scripting("<p>storage</p>", true);
    let runtime = ScriptRuntime::new(dom.document.clone(), "https://example.test/");
    let mut host = runtime.host.borrow_mut();
    let key: StorageString = "key".into();
    let first: StorageString = "a".repeat(4 * 1024 * 1024).into();
    let second: StorageString = "b".repeat(4 * 1024 * 1024).into();
    for index in 0..7 {
        host.storage_set(
            StorageAreaKind::Local,
            key.clone(),
            if index % 2 == 0 {
                first.clone()
            } else {
                second.clone()
            },
        )
        .unwrap();
    }
    assert!(matches!(
        host.storage_set(StorageAreaKind::Local, key.clone(), second),
        Err(StorageError::QuotaExceeded)
    ));
    assert_eq!(host.storage_get(StorageAreaKind::Local, &key), Some(&first));
    host.storage_set(StorageAreaKind::Local, key.clone(), first)
        .unwrap();
    host.storage_remove(StorageAreaKind::Local, key).unwrap();
    host.storage_clear(StorageAreaKind::Local).unwrap();
    assert_eq!(host.storage_len(StorageAreaKind::Local), 0);
}

#[test]
fn quota_exception_options_and_structured_clone_keep_their_contract() {
    let dom = dom::parse_with_scripting(
        r#"<p></p><script>
        const check = value => { if (!value) throw new Error('quota contract'); };
        const error = new QuotaExceededError('full', { quota: 10, requested: 11 });
        const copy = structuredClone(error);
        check(copy instanceof QuotaExceededError && copy instanceof DOMException);
        check(copy.name === 'QuotaExceededError' && copy.code === 22 && copy.message === 'full');
        check(copy.quota === 10 && copy.requested === 11);
        const empty = structuredClone(new QuotaExceededError());
        check(empty.quota === null && empty.requested === null);
        try { new QuotaExceededError('', { quota: Infinity }); throw new Error('missing TypeError'); }
        catch (error) { check(error instanceof TypeError); }
        document.querySelector('p').textContent = 'passed';
    </script>"#,
        true,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.test/");
    let result = runtime.execute_initial(&script_inputs(&dom));
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert_eq!(
        dom.elements_named("p").next().unwrap().text_content(),
        "passed"
    );
}
