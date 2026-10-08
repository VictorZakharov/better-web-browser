//! Consume only independent byte allocations created by value_from_v8.
//! No author buffer is retained or borrowed mutably by a native painter.
use super::JsValue;

pub(super) fn take(args: &mut [JsValue], index: usize) -> Option<Vec<u8>> {
    let value = args.get_mut(index)?;
    if !matches!(value, JsValue::Bytes(_)) {
        return None;
    }
    let JsValue::Bytes(bytes) = std::mem::replace(value, JsValue::Null) else {
        unreachable!("checked native bytes before taking ownership");
    };
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn taking_bytes_preserves_allocation_and_other_arguments() {
        let mut args = [
            JsValue::from(7),
            JsValue::Bytes(vec![11; 1024]),
            JsValue::Bytes(vec![19; 8]),
        ];
        let pointer = args[1].as_bytes().unwrap().as_ptr();
        let original_tail = args[2].clone();
        let bytes = take(&mut args, 1).unwrap();
        assert_eq!(bytes.as_ptr(), pointer);
        assert_eq!(bytes, vec![11; 1024]);
        assert_eq!(args[1], JsValue::Null);
        assert_eq!(args[2], original_tail);
        assert_eq!(args[0], JsValue::from(7));
    }

    #[test]
    fn absent_or_nonbyte_inputs_are_unchanged() {
        for input in [
            JsValue::Null,
            JsValue::Undefined,
            JsValue::from(3),
            JsValue::from("bytes".to_owned()),
            JsValue::Array(vec![]),
        ] {
            let mut args = [input.clone()];
            assert!(take(&mut args, 0).is_none());
            assert_eq!(args[0], input);
            assert!(take(&mut args, 1).is_none());
            assert_eq!(args[0], input);
        }
        assert_eq!(take(&mut [], 0), None);
    }
}
