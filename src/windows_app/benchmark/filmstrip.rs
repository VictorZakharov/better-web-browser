//! Navigation-start-anchored hidden screenshots for visible startup comparisons.

use super::*;

const CAPTURE_GRACE: Duration = Duration::from_millis(250);
const MAX_FILMSTRIP_FRAMES: usize = 120;

pub(in crate::windows_app) struct Filmstrip {
    directory: PathBuf,
    pub(super) interval: Duration,
    pub(super) duration: Duration,
    pub(super) frame_count: usize,
    scheduled: bool,
    frames: Vec<Frame>,
}

struct Frame {
    scheduled_ms: u64,
    captured_ms: f64,
    file: String,
    error: Option<String>,
}

impl Filmstrip {
    pub(super) fn new(
        directory: PathBuf,
        interval: Duration,
        duration: Duration,
    ) -> Result<Self, String> {
        let interval_ms = interval.as_millis();
        let duration_ms = duration.as_millis();
        if interval_ms == 0 || duration_ms == 0 || !duration_ms.is_multiple_of(interval_ms) {
            return Err("filmstrip duration must be a positive multiple of its interval".into());
        }
        let frame_count = usize::try_from(duration_ms / interval_ms)
            .map_err(|_| "filmstrip frame count is too large")?;
        if frame_count == 0 || frame_count > MAX_FILMSTRIP_FRAMES {
            return Err(format!(
                "filmstrip requires between 1 and {MAX_FILMSTRIP_FRAMES} frames"
            ));
        }
        Ok(Self {
            directory,
            interval,
            duration,
            frame_count,
            scheduled: false,
            frames: Vec::with_capacity(frame_count),
        })
    }

    pub(super) fn remaining(&self, navigation_started: Instant) -> Duration {
        (navigation_started + self.duration + CAPTURE_GRACE)
            .saturating_duration_since(Instant::now())
    }

    fn path(&self, index: usize) -> Option<(PathBuf, u64)> {
        if index == 0 || index > self.frame_count {
            return None;
        }
        let scheduled_ms = self.interval.as_millis() as u64 * index as u64;
        Some((
            self.directory
                .join(format!("frame-{scheduled_ms:06}ms.png")),
            scheduled_ms,
        ))
    }

    fn record(&mut self, frame: Frame) -> Result<(), String> {
        self.frames.push(frame);
        self.frames.sort_by_key(|frame| frame.scheduled_ms);
        let frames = self
            .frames
            .iter()
            .map(|frame| {
                format!(
                    "    {{\"scheduled_ms\": {}, \"captured_ms\": {:.3}, \"file\": {}, \"error\": {}}}",
                    frame.scheduled_ms,
                    frame.captured_ms,
                    json_string(&frame.file),
                    frame
                        .error
                        .as_deref()
                        .map(json_string)
                        .unwrap_or_else(|| "null".into())
                )
            })
            .collect::<Vec<_>>()
            .join(",\n");
        let manifest = format!(
            "{{\n  \"anchor\": \"navigation_start\",\n  \"interval_ms\": {},\n  \"duration_ms\": {},\n  \"frames\": [\n{}\n  ]\n}}\n",
            self.interval.as_millis(),
            self.duration.as_millis(),
            frames
        );
        std::fs::write(self.directory.join("manifest.json"), manifest)
            .map_err(|error| format!("write filmstrip manifest: {error}"))
    }
}

impl BrowserState {
    pub(in crate::windows_app) fn schedule_benchmark_filmstrip(&mut self) {
        let Some(benchmark) = self.benchmark.as_mut() else {
            return;
        };
        let Some(navigation_started) = benchmark.navigation_started else {
            return;
        };
        let Some(filmstrip) = benchmark.filmstrip.as_mut() else {
            return;
        };
        if filmstrip.scheduled {
            return;
        }
        if let Err(error) = std::fs::create_dir_all(&filmstrip.directory) {
            benchmark.error = Some(format!("create filmstrip directory: {error}"));
            return;
        }
        filmstrip.scheduled = true;
        let window = self.window as usize;
        let interval = filmstrip.interval;
        let frame_count = filmstrip.frame_count;
        std::thread::spawn(move || {
            for index in 1..=frame_count {
                let deadline = navigation_started + interval * index as u32;
                std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
                unsafe {
                    PostMessageW(window as Hwnd, WM_APP_BENCHMARK_FILMSTRIP, index, 0);
                }
            }
        });
    }

    pub(in crate::windows_app) unsafe fn capture_benchmark_filmstrip_frame(
        &mut self,
        index: usize,
    ) {
        let Some(navigation_started) = self
            .benchmark
            .as_ref()
            .and_then(|benchmark| benchmark.navigation_started)
        else {
            return;
        };
        let Some((path, scheduled_ms)) = self
            .benchmark
            .as_ref()
            .and_then(|benchmark| benchmark.filmstrip.as_ref())
            .and_then(|filmstrip| filmstrip.path(index))
        else {
            return;
        };
        let captured_ms = navigation_started.elapsed().as_secs_f64() * 1_000.0;
        let error = self.capture_screenshot(&path).err();
        let file = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        if let Some(filmstrip) = self
            .benchmark
            .as_mut()
            .and_then(|benchmark| benchmark.filmstrip.as_mut())
            && let Err(error) = filmstrip.record(Frame {
                scheduled_ms,
                captured_ms,
                file,
                error,
            })
            && let Some(benchmark) = self.benchmark.as_mut()
        {
            benchmark.error.get_or_insert(error);
        }
    }
}
