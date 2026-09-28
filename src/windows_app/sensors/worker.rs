//! Bounded sampling scheduler. Device access exists only behind the authorized worker commands.

use super::{SensorOwner, TabId};
use better_web_browser::renderer_process::{SensorSinkError, SensorUpdateSink};
use better_web_browser::renderer_protocol::{
    SensorAction, SensorError, SensorEvent, SensorKind, SensorReading, SensorRequest, SensorUpdate,
};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const MAX_STREAMS: usize = 32;
const POLL_INTERVAL: Duration = Duration::from_millis(10);

#[cfg(test)]
mod extended_tests;
#[cfg(test)]
mod tests;

pub(super) enum Command<S = SensorUpdateSink> {
    Request(SensorOwner, SensorRequest, S),
    RetireTab(TabId),
    Shutdown,
}

#[derive(Clone, Debug)]
pub(super) struct Sample {
    pub stamp: i64,
    pub reading: SensorReading,
    /// Raw lux remains browser-owned; only 50-lux quantized readings cross IPC.
    pub raw_light_lux: Option<f64>,
}

pub(super) trait SensorProvider {
    fn supported(&mut self, kind: SensorKind) -> Result<bool, SensorError>;
    fn set_active(&mut self, active: bool);
    fn sample(&mut self, kind: SensorKind, interval_ms: f64)
    -> Result<Option<Sample>, SensorError>;
}

pub(super) trait EventSink: Send + 'static {
    fn try_emit(&self, update: SensorUpdate) -> Result<(), SensorSinkError>;
}

impl EventSink for SensorUpdateSink {
    fn try_emit(&self, update: SensorUpdate) -> Result<(), SensorSinkError> {
        self.try_send(update)
    }
}

struct Stream<S> {
    owner: SensorOwner,
    kind: SensorKind,
    request_id: u64,
    sink: S,
    interval: Duration,
    next_due: Instant,
    last_stamp: Option<i64>,
    last_light: Option<(f64, f64)>,
}

pub(super) fn is_retired(retired: &Mutex<HashMap<TabId, (u64, u64)>>, owner: SensorOwner) -> bool {
    retired
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .get(&owner.tab)
        .is_some_and(|latest| *latest >= (owner.session_id, owner.document.get()))
}

pub(super) fn run<P: SensorProvider, S: EventSink>(
    incoming: Receiver<Command<S>>,
    retired: Arc<Mutex<HashMap<TabId, (u64, u64)>>>,
    visible_tab: Arc<AtomicU64>,
    stopping: Arc<AtomicBool>,
    mut provider: P,
) {
    let mut streams = HashMap::<(SensorOwner, u64), Stream<S>>::new();
    let mut active = false;
    loop {
        if stopping.load(Ordering::Acquire) {
            break;
        }
        match incoming.recv_timeout(POLL_INTERVAL) {
            Ok(Command::Request(owner, request, sink)) => {
                handle_request(
                    &mut streams,
                    &mut provider,
                    &retired,
                    &visible_tab,
                    active,
                    (owner, request, sink),
                );
            }
            Ok(Command::RetireTab(tab)) => streams.retain(|(candidate, _), _| candidate.tab != tab),
            Ok(Command::Shutdown) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        // Cancellation can be published while recv_timeout sleeps. Recheck immediately
        // before touching the provider so a retired document never gets another sample.
        streams.retain(|(owner, _), _| !is_retired(&retired, *owner));
        let visible = visible_tab.load(Ordering::Acquire);
        let should_activate = visible != 0
            && streams
                .values()
                .any(|stream| stream.owner.tab.get() == visible);
        if should_activate != active {
            provider.set_active(should_activate);
            active = should_activate;
        }
        if !active {
            continue;
        }
        let now = Instant::now();
        let mut failures = Vec::new();
        for (key, stream) in &mut streams {
            if stream.owner.tab.get() != visible
                || visible_tab.load(Ordering::Acquire) != visible
                || is_retired(&retired, stream.owner)
                || now < stream.next_due
            {
                continue;
            }
            stream.next_due = now + stream.interval;
            let sample = provider.sample(stream.kind, stream.interval.as_secs_f64() * 1000.0);
            if visible_tab.load(Ordering::Acquire) != visible || is_retired(&retired, stream.owner)
            {
                continue;
            }
            match sample {
                Ok(Some(sample)) if stream.last_stamp != Some(sample.stamp) => {
                    stream.last_stamp = Some(sample.stamp);
                    let light = if let Some(raw) = sample.raw_light_lux {
                        let SensorReading::Illuminance { illuminance, .. } = &sample.reading else {
                            let _ = emit(stream, SensorEvent::Error(SensorError::NotReadable));
                            failures.push(*key);
                            continue;
                        };
                        if !raw.is_finite() || raw < 0.0 {
                            let _ = emit(stream, SensorEvent::Error(SensorError::NotReadable));
                            failures.push(*key);
                            continue;
                        }
                        if !light_changed(stream.last_light, raw, *illuminance) {
                            continue;
                        }
                        Some((raw, *illuminance))
                    } else {
                        None
                    };
                    // A full renderer sample lane loses this sample, never blocks sensor polling.
                    match emit(stream, SensorEvent::Reading(sample.reading)) {
                        Ok(()) => {
                            if let Some(light) = light {
                                stream.last_light = Some(light);
                            }
                        }
                        Err(SensorSinkError::Full | SensorSinkError::Hidden) => {}
                        Err(SensorSinkError::Disconnected) => {
                            failures.push(*key);
                        }
                        Err(SensorSinkError::Invalid(_)) => {
                            let _ = emit(stream, SensorEvent::Error(SensorError::NotReadable));
                            failures.push(*key);
                        }
                    }
                }
                Ok(_) => {}
                Err(error) => {
                    let _ = emit(stream, SensorEvent::Error(error));
                    failures.push(*key);
                }
            }
        }
        for key in failures {
            streams.remove(&key);
        }
    }
    if active {
        provider.set_active(false);
    }
}

fn light_changed(previous: Option<(f64, f64)>, raw: f64, quantized: f64) -> bool {
    previous.is_none_or(|(previous_raw, previous_quantized)| {
        // W3C Ambient Light §6.2: both a >=25-lux raw change and a changed
        // 50-lux bucket are necessary. Raw lux never leaves the browser.
        (raw - previous_raw).abs() >= 25.0 && quantized != previous_quantized
    })
}

fn handle_request<P: SensorProvider, S: EventSink>(
    streams: &mut HashMap<(SensorOwner, u64), Stream<S>>,
    provider: &mut P,
    retired: &Mutex<HashMap<TabId, (u64, u64)>>,
    visible_tab: &AtomicU64,
    active: bool,
    (owner, request, sink): (SensorOwner, SensorRequest, S),
) {
    let SensorRequest {
        request_id, action, ..
    } = request;
    match action {
        SensorAction::Stop { .. } => {
            streams.remove(&(owner, request_id));
        }
        SensorAction::Start { kind, frequency_hz } => {
            let mut candidate = Stream {
                owner,
                kind,
                request_id,
                sink,
                interval: sample_interval(kind, frequency_hz),
                next_due: Instant::now(),
                last_stamp: None,
                last_light: None,
            };
            if is_retired(retired, owner) || visible_tab.load(Ordering::Acquire) != owner.tab.get()
            {
                let _ = emit(&candidate, SensorEvent::Error(SensorError::NotAllowed));
                return;
            }
            if streams.contains_key(&(owner, request_id)) || streams.len() >= MAX_STREAMS {
                let _ = emit(&candidate, SensorEvent::Error(SensorError::NotReadable));
                return;
            }
            match provider.supported(kind) {
                Ok(true) => {
                    // A device discovery may have blocked while the tab was hidden or
                    // its renderer was retired; do not activate that stale request.
                    if is_retired(retired, owner)
                        || visible_tab.load(Ordering::Acquire) != owner.tab.get()
                    {
                        let _ = emit(&candidate, SensorEvent::Error(SensorError::NotAllowed));
                        return;
                    }
                    // supported() can lazily open a second WinRT device while other
                    // streams are already active. Apply the report interval to it.
                    if active {
                        provider.set_active(true);
                    }
                    // An activation that cannot reach the renderer must not retain a
                    // live hardware stream with a permanently activating JS object.
                    if emit(&candidate, SensorEvent::Activated).is_err() {
                        return;
                    }
                    candidate.next_due = Instant::now();
                    streams.insert((owner, request_id), candidate);
                }
                Ok(false) => {
                    let _ = emit(&candidate, SensorEvent::Error(SensorError::NotSupported));
                }
                Err(error) => {
                    let _ = emit(&candidate, SensorEvent::Error(error));
                }
            }
        }
        SensorAction::RequestPermission { .. } => {
            // Browser UI resolves permission before admitting a worker command.
        }
    }
}

fn sample_interval(kind: SensorKind, frequency_hz: Option<f64>) -> Duration {
    let frequency = frequency_hz.unwrap_or(match kind {
        SensorKind::Orientation | SensorKind::OrientationAbsoluteLegacy | SensorKind::Motion => {
            20.0
        }
        SensorKind::Accelerometer
        | SensorKind::LinearAcceleration
        | SensorKind::Gravity
        | SensorKind::Gyroscope
        | SensorKind::Magnetometer
        | SensorKind::AbsoluteOrientation
        | SensorKind::RelativeOrientation => 30.0,
        SensorKind::AmbientLight => 10.0,
    });
    Duration::from_secs_f64(1.0 / frequency.clamp(1.0, 50.0))
}

fn emit<S: EventSink>(stream: &Stream<S>, event: SensorEvent) -> Result<(), SensorSinkError> {
    stream.sink.try_emit(SensorUpdate {
        document: stream.owner.document,
        request_id: stream.request_id,
        event,
    })
}
