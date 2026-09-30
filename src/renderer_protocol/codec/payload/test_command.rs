use crate::renderer_protocol::{ProtocolError, TestCommand};

pub(super) fn decode(payload: &[u8]) -> Result<TestCommand, ProtocolError> {
    match payload {
        [10] => Ok(TestCommand::InternalError),
        [11] => Ok(TestCommand::DocumentError),
        [1] => Ok(TestCommand::Crash),
        [2] => Ok(TestCommand::Hang),
        [3] => Ok(TestCommand::WriteMalformedFrame),
        [4, low, high] => Ok(TestCommand::ProbeRestrictions {
            loopback_port: u16::from_le_bytes([*low, *high]),
        }),
        [5] => Ok(TestCommand::AccessViolation),
        [6] => Ok(TestCommand::OutOfMemory),
        [7] => Ok(TestCommand::StackOverflow),
        [8, low, high] => Ok(TestCommand::DelayCommandRead {
            millis: u16::from_le_bytes([*low, *high]),
        }),
        [9, low, high, padding @ ..]
            if padding.len() == usize::from(u16::from_le_bytes([*low, *high]))
                && padding.iter().all(|byte| *byte == 0) =>
        {
            Ok(TestCommand::Padding {
                bytes: u16::from_le_bytes([*low, *high]),
            })
        }
        _ => Err(ProtocolError::InvalidPayload("test command")),
    }
}
