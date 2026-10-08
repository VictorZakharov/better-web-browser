use super::*;

#[test]
fn owned_reply_transfers_storage_while_borrowed_reply_copies() {
    crate::engine::script::engine::runtime::initialize_v8();
    let mut isolate = v8::Isolate::new(v8::CreateParams::default());
    v8::scope!(let scope, &mut isolate);
    let context = v8::Context::new(scope, Default::default());
    let scope = &mut v8::ContextScope::new(scope, context);
    let original = vec![17u8, 39, 81, 127];
    let pointer = original.as_ptr();
    let owned = value_to_v8_owned(scope, JsValue::Bytes(original)).unwrap();
    let view = v8::Local::<v8::Uint8Array>::try_from(owned).unwrap();
    let buffer = view.buffer(scope).unwrap();
    assert_eq!(
        buffer
            .get_backing_store()
            .data()
            .unwrap()
            .as_ptr()
            .cast_const()
            .cast::<u8>(),
        pointer
    );
    assert_eq!(
        value_from_v8(scope, owned).unwrap(),
        JsValue::Bytes(vec![17, 39, 81, 127])
    );

    let borrowed = JsValue::Bytes(vec![19, 91, 213, 128]);
    let before = borrowed.clone();
    let returned = value_to_v8(scope, &borrowed).unwrap();
    let view = v8::Local::<v8::Uint8Array>::try_from(returned).unwrap();
    let buffer = view.buffer(scope).unwrap();
    assert_ne!(
        buffer
            .get_backing_store()
            .data()
            .unwrap()
            .as_ptr()
            .cast_const()
            .cast::<u8>(),
        borrowed.as_bytes().unwrap().as_ptr()
    );
    assert_eq!(value_from_v8(scope, returned).unwrap(), before);
    assert_eq!(borrowed, before);
}

#[test]
fn nested_and_empty_owned_bytes_round_trip_through_javascript() {
    crate::engine::script::engine::runtime::initialize_v8();
    let mut isolate = v8::Isolate::new(v8::CreateParams::default());
    v8::scope!(let scope, &mut isolate);
    let context = v8::Context::new(scope, Default::default());
    let scope = &mut v8::ContextScope::new(scope, context);
    let expected = JsValue::Array(vec![
        JsValue::Bytes(vec![]),
        JsValue::Array(vec![JsValue::Bytes(vec![0, 255, 128])]),
    ]);
    let returned = value_to_v8_owned(scope, expected.clone()).unwrap();
    assert_eq!(value_from_v8(scope, returned).unwrap(), expected);
}

#[test]
fn incoming_subviews_copy_only_their_bytes_and_do_not_alias_author_memory() {
    crate::engine::script::engine::runtime::initialize_v8();
    let mut isolate = v8::Isolate::new(v8::CreateParams::default());
    v8::scope!(let scope, &mut isolate);
    let context = v8::Context::new(scope, Default::default());
    let scope = &mut v8::ContextScope::new(scope, context);
    let array = value_to_v8_owned(scope, JsValue::Bytes(vec![9, 17, 39, 81, 127, 8])).unwrap();
    let array = v8::Local::<v8::Uint8Array>::try_from(array).unwrap();
    let buffer = array.buffer(scope).unwrap();
    let view = v8::Uint8Array::new(scope, buffer, 1, 4).unwrap();
    let JsValue::Bytes(mut copied) = value_from_v8(scope, view.into()).unwrap() else {
        panic!("not bytes");
    };
    assert_eq!(copied, [17, 39, 81, 127]);
    copied.fill(251);
    assert_eq!(
        value_from_v8(scope, array.into()).unwrap(),
        JsValue::Bytes(vec![9, 17, 39, 81, 127, 8])
    );
}

#[test]
fn object_reply_owns_its_bytes_after_collection_and_does_not_alias_siblings() {
    crate::engine::script::engine::runtime::initialize_v8();
    let mut isolate = v8::Isolate::new(v8::CreateParams::default());
    v8::scope!(let scope, &mut isolate);
    let context = v8::Context::new(scope, Default::default());
    let scope = &mut v8::ContextScope::new(scope, context);
    let returned = value_to_v8_owned(
        scope,
        JsValue::Object(vec![
            ("first".into(), JsValue::Bytes(vec![1, 2, 3])),
            ("second".into(), JsValue::Bytes(vec![4, 5, 6])),
        ]),
    )
    .unwrap();
    scope.low_memory_notification();
    let object = v8::Local::<v8::Object>::try_from(returned).unwrap();
    let first = v8::String::new(scope, "first").unwrap();
    let first = object.get(scope, first.into()).unwrap();
    let second = v8::String::new(scope, "second").unwrap();
    let second = object.get(scope, second.into()).unwrap();
    assert_eq!(
        value_from_v8(scope, first).unwrap(),
        JsValue::Bytes(vec![1, 2, 3])
    );
    assert_eq!(
        value_from_v8(scope, second).unwrap(),
        JsValue::Bytes(vec![4, 5, 6])
    );
}
