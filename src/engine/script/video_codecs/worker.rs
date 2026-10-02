//! All rav1d context creation and decoding occurs inside the contained worker.
use super::*;
use crate::engine::image_decode::VideoAv1Decoder;
pub(super) fn run(
    _config: Config,
    commands: mpsc::Receiver<Command>,
    results: mpsc::SyncSender<Result<Vec<Output>, String>>,
    cancelled: &AtomicBool,
) {
    let mut decoder = match VideoAv1Decoder::new() {
        Ok(value) => value,
        Err(error) => {
            let _ = results.send(Err(error));
            return;
        }
    };
    if cancelled.load(Ordering::Acquire) || results.send(Ok(Vec::new())).is_err() {
        return;
    }
    while let Ok(command) = commands.recv() {
        if cancelled.load(Ordering::Acquire) {
            return;
        }
        let decoded = match command {
            Command::Input {
                bytes,
                timestamp,
                duration,
                key,
            } => decoder.decode(&bytes, timestamp, duration, key, cancelled),
            Command::Flush => decoder.flush(cancelled),
        };
        let result = decoded.map(|pictures| {
            pictures
                .into_iter()
                .map(|picture| Output {
                    bytes: picture.image.rgba,
                    width: picture.image.width,
                    height: picture.image.height,
                    timestamp: picture.timestamp,
                    duration: picture.duration,
                })
                .collect()
        });
        let failed = result.is_err();
        if cancelled.load(Ordering::Acquire) || results.send(result).is_err() || failed {
            return;
        }
    }
}
