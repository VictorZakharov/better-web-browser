//! Large native pixel replies remain owned bytes all the way to the realm's
//! typed array. Serializing every byte as a JSON number multiplies allocations
//! and parsing work without adding validation or isolation.
use super::{BackendContexts, Command, MAX_UPLOAD_BYTES, gl};

pub(crate) enum PixelReply {
    Bytes(Vec<u8>),
    Error,
    Lost,
}

impl BackendContexts {
    pub(super) fn read_pixels_owned(
        &mut self,
        id: u32,
        command: &str,
        input: Option<Vec<u8>>,
    ) -> PixelReply {
        self.read_pixels_data(id, command, input.map(std::borrow::Cow::Owned))
    }

    fn read_pixels_data(
        &mut self,
        id: u32,
        command: &str,
        input: Option<std::borrow::Cow<'_, [u8]>>,
    ) -> PixelReply {
        let other_bytes: usize = self
            .contexts
            .iter()
            .filter(|(key, _)| **key != id)
            .map(|(_, context)| context.resource_bytes)
            .sum();
        let Some(context) = self.contexts.get_mut(&id) else {
            return PixelReply::Lost;
        };
        context.resource_limit = super::MAX_RESOURCE_BYTES
            .min(super::MAX_PROCESS_RESOURCE_BYTES.saturating_sub(other_bytes));
        if command.len() > 1024
            || input
                .as_ref()
                .is_some_and(|bytes| bytes.len() > MAX_UPLOAD_BYTES)
        {
            context.error(gl::OUT_OF_MEMORY);
            return PixelReply::Error;
        }
        let command = match serde_json::from_str::<Command>(command) {
            Ok(command) => command,
            Err(_) => {
                context.error(gl::INVALID_VALUE);
                return PixelReply::Error;
            }
        };
        // This host operation can never dispatch an arbitrary native command.
        if !["readPixels", "getBufferSubData"].contains(&command.op.as_str()) {
            context.error(gl::INVALID_OPERATION);
            return PixelReply::Error;
        }
        if context.native.make_current().is_err() {
            return PixelReply::Lost;
        }
        let result = if command.op == "getBufferSubData" {
            context.read_buffer(&command)
        } else {
            context.read_pixels_data(&command, input)
        };
        match result {
            Ok(bytes) => PixelReply::Bytes(bytes),
            Err(error) => {
                context.error(error);
                PixelReply::Error
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::webgl::json;
    use crate::engine::webgl::tests::{command, context};

    #[test]
    fn binary_readback_preserves_owned_pixels_and_rejects_non_read_commands() {
        let (mut contexts, id) = context();
        command(
            &mut contexts,
            id,
            "clearColor",
            &[],
            &[1.0, 0.0, 0.0, 1.0],
            "",
            None,
        );
        command(
            &mut contexts,
            id,
            "clear",
            &[gl::COLOR_BUFFER_BIT],
            &[],
            "",
            None,
        );
        let read = r#"{"op":"readPixels","i":[0,0,1,1,6408,5121,4]}"#;
        let PixelReply::Bytes(bytes) = contexts.read_pixels(id, read, Some(&[9, 8, 7, 6])) else {
            panic!("native byte reply missing");
        };
        assert_eq!(bytes, [255, 0, 0, 255]);
        assert!(matches!(
            contexts.read_pixels(id, r#"{"op":"clear","i":[16384]}"#, None),
            PixelReply::Error
        ));
        assert_eq!(
            command(&mut contexts, id, "getError", &[], &[], "", None),
            json!(gl::INVALID_OPERATION)
        );
        let PixelReply::Bytes(bytes) = contexts.read_pixels(id, read, None) else {
            panic!("read after rejection")
        };
        assert_eq!(bytes, [255, 0, 0, 255]);
        assert!(matches!(
            contexts.read_pixels(u32::MAX, read, None),
            PixelReply::Lost
        ));
    }

    #[test]
    fn binary_readback_checks_payloads_before_native_memory_access() {
        let (mut contexts, id) = context();
        for (text, expected) in [
            (
                r#"{"op":"readPixels","i":[0,0,2147483648,1,6408,5121,4]}"#,
                gl::INVALID_VALUE,
            ),
            (
                r#"{"op":"readPixels","i":[0,0,1,1,6408,5121,3]}"#,
                gl::INVALID_OPERATION,
            ),
            (
                r#"{"op":"readPixels","native_pointer":1}"#,
                gl::INVALID_VALUE,
            ),
            ("not JSON", gl::INVALID_VALUE),
        ] {
            assert!(matches!(
                contexts.read_pixels(id, text, None),
                PixelReply::Error
            ));
            assert_eq!(
                command(&mut contexts, id, "getError", &[], &[], "", None),
                json!(expected)
            );
        }
        assert!(matches!(
            contexts.read_pixels(id, &" ".repeat(1025), None),
            PixelReply::Error
        ));
        assert_eq!(
            command(&mut contexts, id, "getError", &[], &[], "", None),
            json!(gl::OUT_OF_MEMORY)
        );
        let read = r#"{"op":"readPixels","i":[0,0,1,1,6408,5121,4]}"#;
        assert!(matches!(
            contexts.read_pixels(id, read, Some(&[1, 2, 3])),
            PixelReply::Error
        ));
        assert_eq!(
            command(&mut contexts, id, "getError", &[], &[], "", None),
            json!(gl::INVALID_OPERATION)
        );
    }
}
