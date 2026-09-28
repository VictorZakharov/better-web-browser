use super::{
    BrowserCaptureMessage, CaptureContainmentReport, CaptureDevices, CaptureFailure, CaptureSample,
    CaptureSampleKind, CaptureSessionId, HEADER_LENGTH, MAGIC, MAX_CONTROL_BYTES, MAX_SAMPLE_BYTES,
    PROTOCOL_MAJOR, PROTOCOL_MINOR, WorkerCaptureMessage,
};
use crate::renderer_protocol::Nonce;
use std::fmt;
use std::io::{Read, Write};

const HELLO: u16 = 1;
const START: u16 = 2;
const STOP: u16 = 3;
const SHUTDOWN: u16 = 4;
const READY: u16 = 5;
const STARTED: u16 = 6;
const STOPPED: u16 = 7;
const FAILED: u16 = 8;
const SHUTDOWN_COMPLETE: u16 = 9;
const SAMPLE: u16 = 10;
const SAMPLE_PREFIX_BYTES: usize = 49;

#[derive(Debug)]
pub(crate) enum CaptureProtocolError {
    Io(std::io::Error),
    InvalidMagic,
    IncompatibleVersion,
    InvalidFlags,
    PayloadTooLarge,
    WrongSession,
    WrongSequence,
    SequenceExhausted,
    UnexpectedMessage,
    InvalidPayload(&'static str),
}

impl fmt::Display for CaptureProtocolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "capture IPC I/O failed: {error}"),
            Self::InvalidMagic => formatter.write_str("invalid capture IPC magic"),
            Self::IncompatibleVersion => formatter.write_str("incompatible capture IPC version"),
            Self::InvalidFlags => formatter.write_str("reserved capture IPC flags"),
            Self::PayloadTooLarge => formatter.write_str("capture IPC payload exceeds its limit"),
            Self::WrongSession => formatter.write_str("stale capture IPC session"),
            Self::WrongSequence => formatter.write_str("out-of-order capture IPC sequence"),
            Self::SequenceExhausted => formatter.write_str("capture IPC sequence exhausted"),
            Self::UnexpectedMessage => formatter.write_str("capture IPC message on wrong pipe"),
            Self::InvalidPayload(field) => write!(formatter, "invalid capture IPC {field}"),
        }
    }
}

impl std::error::Error for CaptureProtocolError {}

impl From<std::io::Error> for CaptureProtocolError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

pub(crate) struct CaptureFrameWriter<W> {
    inner: W,
    session: CaptureSessionId,
    next_sequence: u64,
}

impl<W: Write> CaptureFrameWriter<W> {
    pub(crate) fn new(inner: W, session: CaptureSessionId) -> Self {
        Self {
            inner,
            session,
            next_sequence: 1,
        }
    }

    pub(crate) fn send_browser(
        &mut self,
        message: &BrowserCaptureMessage,
    ) -> Result<(), CaptureProtocolError> {
        let (kind, payload) = match message {
            BrowserCaptureMessage::Hello { nonce, devices } => {
                devices.validate()?;
                let mut bytes = nonce.as_bytes().to_vec();
                bytes.push(devices.bits());
                (HELLO, bytes)
            }
            BrowserCaptureMessage::Start { capture_id } => {
                (START, nonzero(*capture_id)?.to_le_bytes().to_vec())
            }
            BrowserCaptureMessage::Stop { capture_id } => {
                (STOP, nonzero(*capture_id)?.to_le_bytes().to_vec())
            }
            BrowserCaptureMessage::Shutdown => (SHUTDOWN, Vec::new()),
        };
        self.write_frame(kind, &payload)
    }

    pub(crate) fn send_worker(
        &mut self,
        message: &WorkerCaptureMessage,
    ) -> Result<(), CaptureProtocolError> {
        let (kind, payload) = match message {
            WorkerCaptureMessage::Ready { nonce, containment } => {
                let mut bytes = nonce.as_bytes().to_vec();
                bytes.extend_from_slice(&[
                    containment.app_container.into(),
                    containment.no_console_window.into(),
                    containment.minimal_environment.into(),
                    containment.camera_capability.into(),
                    containment.microphone_capability.into(),
                ]);
                (READY, bytes)
            }
            WorkerCaptureMessage::Started { capture_id } => {
                (STARTED, nonzero(*capture_id)?.to_le_bytes().to_vec())
            }
            WorkerCaptureMessage::Stopped { capture_id } => {
                (STOPPED, nonzero(*capture_id)?.to_le_bytes().to_vec())
            }
            WorkerCaptureMessage::Failed { capture_id, reason } => {
                let mut bytes = nonzero(*capture_id)?.to_le_bytes().to_vec();
                bytes.push(match reason {
                    CaptureFailure::NoDevice => 1,
                    CaptureFailure::AccessDenied => 2,
                    CaptureFailure::DeviceBusy => 3,
                    CaptureFailure::InvalidFormat => 4,
                    CaptureFailure::Internal => 5,
                });
                (FAILED, bytes)
            }
            WorkerCaptureMessage::ShutdownComplete => (SHUTDOWN_COMPLETE, Vec::new()),
        };
        self.write_frame(kind, &payload)
    }

    pub(crate) fn send_sample(
        &mut self,
        sample: &CaptureSample,
    ) -> Result<(), CaptureProtocolError> {
        sample.validate()?;
        let mut bytes = Vec::with_capacity(SAMPLE_PREFIX_BYTES + sample.bytes.len());
        for value in [
            sample.capture_id,
            sample.track_id,
            sample.sequence,
            sample.timestamp_100ns,
        ] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.push(match sample.kind {
            CaptureSampleKind::VideoNv12 => 1,
            CaptureSampleKind::AudioPcm16 => 2,
        });
        for value in [
            sample.width_or_rate,
            sample.height_or_frames,
            sample.stride_or_channels,
            sample.bytes.len() as u32,
        ] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(&sample.bytes);
        self.write_frame(SAMPLE, &bytes)
    }

    fn write_frame(&mut self, kind: u16, payload: &[u8]) -> Result<(), CaptureProtocolError> {
        let maximum = if kind == SAMPLE {
            SAMPLE_PREFIX_BYTES + MAX_SAMPLE_BYTES
        } else {
            MAX_CONTROL_BYTES
        };
        if payload.len() > maximum {
            return Err(CaptureProtocolError::PayloadTooLarge);
        }
        let sequence = self.next_sequence;
        self.next_sequence = sequence
            .checked_add(1)
            .ok_or(CaptureProtocolError::SequenceExhausted)?;
        let mut header = [0_u8; HEADER_LENGTH];
        header[..4].copy_from_slice(&MAGIC);
        header[4..6].copy_from_slice(&PROTOCOL_MAJOR.to_le_bytes());
        header[6..8].copy_from_slice(&PROTOCOL_MINOR.to_le_bytes());
        header[8..10].copy_from_slice(&kind.to_le_bytes());
        header[12..16].copy_from_slice(&(payload.len() as u32).to_le_bytes());
        header[16..24].copy_from_slice(&self.session.get().to_le_bytes());
        header[24..32].copy_from_slice(&sequence.to_le_bytes());
        self.inner.write_all(&header)?;
        self.inner.write_all(payload)?;
        self.inner.flush()?;
        Ok(())
    }
}

pub(crate) struct CaptureFrameReader<R> {
    inner: R,
    session: CaptureSessionId,
    next_sequence: u64,
}

impl<R: Read> CaptureFrameReader<R> {
    pub(crate) fn new(inner: R, session: CaptureSessionId) -> Self {
        Self {
            inner,
            session,
            next_sequence: 1,
        }
    }

    pub(crate) fn read_browser(&mut self) -> Result<BrowserCaptureMessage, CaptureProtocolError> {
        let (kind, bytes) = self.read_frame(false)?;
        match kind {
            HELLO if bytes.len() == 33 => {
                let nonce = Nonce::new(bytes[..32].try_into().expect("checked length"));
                let devices = CaptureDevices::from_bits(bytes[32])?;
                Ok(BrowserCaptureMessage::Hello { nonce, devices })
            }
            START if bytes.len() == 8 => Ok(BrowserCaptureMessage::Start {
                capture_id: nonzero(u64_at(&bytes, 0))?,
            }),
            STOP if bytes.len() == 8 => Ok(BrowserCaptureMessage::Stop {
                capture_id: nonzero(u64_at(&bytes, 0))?,
            }),
            SHUTDOWN if bytes.is_empty() => Ok(BrowserCaptureMessage::Shutdown),
            _ => Err(CaptureProtocolError::InvalidPayload("browser message")),
        }
    }

    pub(crate) fn read_worker(&mut self) -> Result<WorkerCaptureMessage, CaptureProtocolError> {
        let (kind, bytes) = self.read_frame(false)?;
        match kind {
            READY if bytes.len() == 37 => Ok(WorkerCaptureMessage::Ready {
                nonce: Nonce::new(bytes[..32].try_into().expect("checked length")),
                containment: CaptureContainmentReport {
                    app_container: boolean(bytes[32])?,
                    no_console_window: boolean(bytes[33])?,
                    minimal_environment: boolean(bytes[34])?,
                    camera_capability: boolean(bytes[35])?,
                    microphone_capability: boolean(bytes[36])?,
                },
            }),
            STARTED if bytes.len() == 8 => Ok(WorkerCaptureMessage::Started {
                capture_id: nonzero(u64_at(&bytes, 0))?,
            }),
            STOPPED if bytes.len() == 8 => Ok(WorkerCaptureMessage::Stopped {
                capture_id: nonzero(u64_at(&bytes, 0))?,
            }),
            FAILED if bytes.len() == 9 => {
                let reason = match bytes[8] {
                    1 => CaptureFailure::NoDevice,
                    2 => CaptureFailure::AccessDenied,
                    3 => CaptureFailure::DeviceBusy,
                    4 => CaptureFailure::InvalidFormat,
                    5 => CaptureFailure::Internal,
                    _ => return Err(CaptureProtocolError::InvalidPayload("failure reason")),
                };
                Ok(WorkerCaptureMessage::Failed {
                    capture_id: nonzero(u64_at(&bytes, 0))?,
                    reason,
                })
            }
            SHUTDOWN_COMPLETE if bytes.is_empty() => Ok(WorkerCaptureMessage::ShutdownComplete),
            _ => Err(CaptureProtocolError::InvalidPayload("worker message")),
        }
    }

    pub(crate) fn read_sample(&mut self) -> Result<CaptureSample, CaptureProtocolError> {
        let (kind, bytes) = self.read_frame(true)?;
        if kind != SAMPLE || bytes.len() < SAMPLE_PREFIX_BYTES {
            return Err(CaptureProtocolError::InvalidPayload("sample header"));
        }
        let sample_kind = match bytes[32] {
            1 => CaptureSampleKind::VideoNv12,
            2 => CaptureSampleKind::AudioPcm16,
            _ => return Err(CaptureProtocolError::InvalidPayload("sample kind")),
        };
        let length = u32_at(&bytes, 45) as usize;
        if bytes.len() - SAMPLE_PREFIX_BYTES != length {
            return Err(CaptureProtocolError::InvalidPayload("sample length"));
        }
        let sample = CaptureSample {
            capture_id: u64_at(&bytes, 0),
            track_id: u64_at(&bytes, 8),
            sequence: u64_at(&bytes, 16),
            timestamp_100ns: u64_at(&bytes, 24),
            kind: sample_kind,
            width_or_rate: u32_at(&bytes, 33),
            height_or_frames: u32_at(&bytes, 37),
            stride_or_channels: u32_at(&bytes, 41),
            bytes: bytes[SAMPLE_PREFIX_BYTES..].to_vec(),
        };
        sample.validate()?;
        Ok(sample)
    }

    fn read_frame(&mut self, sample_pipe: bool) -> Result<(u16, Vec<u8>), CaptureProtocolError> {
        let mut header = [0_u8; HEADER_LENGTH];
        self.inner.read_exact(&mut header)?;
        if header[..4] != MAGIC {
            return Err(CaptureProtocolError::InvalidMagic);
        }
        if u16_at(&header, 4) != PROTOCOL_MAJOR || u16_at(&header, 6) > PROTOCOL_MINOR {
            return Err(CaptureProtocolError::IncompatibleVersion);
        }
        let kind = u16_at(&header, 8);
        if (kind == SAMPLE) != sample_pipe || !(HELLO..=SAMPLE).contains(&kind) {
            return Err(CaptureProtocolError::UnexpectedMessage);
        }
        if u16_at(&header, 10) != 0 {
            return Err(CaptureProtocolError::InvalidFlags);
        }
        let length = u32_at(&header, 12) as usize;
        let maximum = if sample_pipe {
            SAMPLE_PREFIX_BYTES + MAX_SAMPLE_BYTES
        } else {
            MAX_CONTROL_BYTES
        };
        if length > maximum {
            return Err(CaptureProtocolError::PayloadTooLarge);
        }
        if u64_at(&header, 16) != self.session.get() {
            return Err(CaptureProtocolError::WrongSession);
        }
        if u64_at(&header, 24) != self.next_sequence {
            return Err(CaptureProtocolError::WrongSequence);
        }
        self.next_sequence = self
            .next_sequence
            .checked_add(1)
            .ok_or(CaptureProtocolError::SequenceExhausted)?;
        let mut payload = vec![0_u8; length];
        self.inner.read_exact(&mut payload)?;
        Ok((kind, payload))
    }
}

fn nonzero(value: u64) -> Result<u64, CaptureProtocolError> {
    if value == 0 {
        Err(CaptureProtocolError::InvalidPayload(
            "zero capture identity",
        ))
    } else {
        Ok(value)
    }
}

fn boolean(value: u8) -> Result<bool, CaptureProtocolError> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(CaptureProtocolError::InvalidPayload("containment flag")),
    }
}

fn u16_at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(bytes[offset..offset + 2].try_into().expect("bounded frame"))
}
fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().expect("bounded frame"))
}
fn u64_at(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().expect("bounded frame"))
}
