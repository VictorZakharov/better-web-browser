//! Native stderr is untrusted diagnostics, never renderer protocol data.

use std::fs::File;
use std::io::{Read, Write};
use std::thread::JoinHandle;

const FORWARD_LIMIT: usize = 64 * 1024;

pub(super) fn spawn(input: File) -> Result<JoinHandle<()>, String> {
    std::thread::Builder::new()
        .name("breeze-renderer-native-diagnostics".into())
        .spawn(move || {
            // Keep draining after the output allowance is exhausted. Otherwise a verbose
            // driver can fill its pipe and block rendering indefinitely. EOF follows job
            // termination; the broker joins this thread after closing the renderer job.
            let mut sink = std::io::stderr();
            let _ = drain(input, &mut sink, FORWARD_LIMIT);
        })
        .map_err(|error| format!("start renderer diagnostics reader: {error}"))
}

fn drain(mut input: impl Read, mut output: impl Write, allowance: usize) -> std::io::Result<()> {
    let mut remaining = allowance;
    let mut buffer = [0_u8; 4096];
    loop {
        let count = match input.read(&mut buffer) {
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            result => result?,
        };
        if count == 0 {
            return Ok(());
        }
        let forward = count.min(remaining);
        if forward != 0 {
            if output.write_all(&buffer[..forward]).is_err() {
                remaining = 0;
            } else {
                remaining -= forward;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verbose_diagnostics_are_fully_drained_but_bounded() {
        let bytes = vec![b'x'; FORWARD_LIMIT * 3];
        let mut input = std::io::Cursor::new(&bytes);
        let mut output = Vec::new();
        drain(&mut input, &mut output, FORWARD_LIMIT).unwrap();
        assert_eq!(input.position(), bytes.len() as u64);
        assert_eq!(output, bytes[..FORWARD_LIMIT]);
    }

    #[test]
    fn unavailable_parent_stderr_does_not_block_drain() {
        struct Broken;
        impl Write for Broken {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(std::io::ErrorKind::BrokenPipe.into())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut input = std::io::Cursor::new(vec![0; FORWARD_LIMIT * 2]);
        drain(&mut input, Broken, FORWARD_LIMIT).unwrap();
        assert_eq!(input.position(), (FORWARD_LIMIT * 2) as u64);
    }

    #[test]
    fn interrupted_and_short_reads_preserve_bytes_without_exceeding_allowance() {
        struct Fragmented {
            data: std::io::Cursor<Vec<u8>>,
            interrupted: bool,
        }
        impl Read for Fragmented {
            fn read(&mut self, target: &mut [u8]) -> std::io::Result<usize> {
                self.interrupted = !self.interrupted;
                if self.interrupted {
                    return Err(std::io::ErrorKind::Interrupted.into());
                }
                let length = target.len().min(3);
                self.data.read(&mut target[..length])
            }
        }
        let mut input = Fragmented {
            data: std::io::Cursor::new(b"diagnostic bytes after an interrupt".to_vec()),
            interrupted: false,
        };
        let mut output = Vec::new();
        drain(&mut input, &mut output, 11).unwrap();
        assert_eq!(output, b"diagnostic ");
        assert_eq!(input.data.position(), input.data.get_ref().len() as u64);
    }

    #[test]
    fn zero_forward_allowance_still_consumes_every_byte() {
        let mut input = std::io::Cursor::new(vec![42; FORWARD_LIMIT + 1]);
        let mut output = Vec::new();
        drain(&mut input, &mut output, 0).unwrap();
        assert!(output.is_empty());
        assert_eq!(input.position(), input.get_ref().len() as u64);
    }
}
