//! Engine: opens streams, installs the real-time processors, and exposes
//! status, meters, health and the latency test to the control plane.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use rtrb::{Consumer, Producer, RingBuffer};
use tuner_audio::resample::RingReader;
use tuner_audio::{
    AudioBackend, CallbackInfo, CaptureCallback, CaptureRequest, DeviceInfo, RenderCallback,
    RenderRequest, StreamHandle, StreamInfo, StreamState, ThreadPriority,
};
use tuner_dsp::{DspMeters, Tuner, TunerConfig};

use crate::controls::SharedControls;
use crate::latency::{analyze_loopback, LatencyProbe, LatencyResult};
use crate::stats::CallbackStats;
use crate::EngineError;

/// Largest block the capture processor handles in one go (bigger device
/// buffers are processed in pieces).
const MAX_BLOCK: usize = 16_384;
/// Monitor gain smoothing (FR-11: ≥ 10 ms).
const MONITOR_GAIN_TAU_S: f32 = 0.015;

/// What to open. Device IDs of `None` mean "system default".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EngineConfig {
    pub capture_device: Option<String>,
    pub monitor_device: Option<String>,
    /// VB-Cable "CABLE Input" endpoint. `None` disables the virtual mic.
    pub cable_device: Option<String>,
    pub allow_exclusive: bool,
    pub allow_asio: bool,
    pub preferred_rate: u32,
    pub seed: u64,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            capture_device: None,
            monitor_device: None,
            cable_device: None,
            allow_exclusive: true,
            allow_asio: true,
            preferred_rate: 48_000,
            seed: 0x5EED,
        }
    }
}

/// Snapshot of what the engine negotiated (FR-02 tier, FR-16 estimate).
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct EngineStatus {
    pub capture: StreamInfo,
    pub monitor: StreamInfo,
    pub cable: Option<StreamInfo>,
    pub cable_error: Option<String>,
    pub monitor_resampling: bool,
    pub dsp_latency_ms: f32,
    /// Capture period + DSP + ring + render period + stream latencies.
    pub estimated_latency_ms: f32,
}

/// Meters and counters for the UI (FR-17, FR-18).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct MeterSnapshot {
    pub dsp: DspMeters,
    pub monitor_fill_ms: f32,
    pub cable_fill_ms: f32,
    /// Cable resampler ratio relative to nominal.
    pub cable_ratio: f32,
    pub xruns: u32,
    pub capture_discontinuities: u32,
    pub monitor_underruns: u32,
    pub cable_underruns: u32,
    pub ring_overflows: u32,
    pub callback_p99_us: u32,
    pub callback_max_us: u32,
    /// Duration of one capture period, the budget each callback must meet.
    pub callback_budget_us: u32,
}

/// Stream health as seen by the control plane.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EngineHealth {
    Healthy,
    CaptureLost,
    MonitorLost,
    CableLost,
    Failed(String),
}

// ───────────────────────────── capture side ─────────────────────────────

struct Processor {
    tuner: Tuner,
    controls: Arc<SharedControls>,
    generation: u64,
    out: Vec<f32>,
    cable_out: Vec<f32>,
    monitor_tx: Producer<f32>,
    cable_tx: Option<Producer<f32>>,
    meters: triple_buffer::Input<DspMeters>,
    stats: Arc<CallbackStats>,
    probe_rx: Consumer<Box<LatencyProbe>>,
    probe_done_tx: Producer<Box<LatencyProbe>>,
    probe: Option<Box<LatencyProbe>>,
    monitor_gain: f32,
    gain_k: f32,
}

impl Processor {
    fn run(&mut self, mono: &[f32], info: &CallbackInfo) {
        let started = Instant::now();
        if info.discontinuity {
            self.stats
                .capture_discontinuities
                .fetch_add(1, Ordering::Relaxed);
        }
        let generation = self.controls.generation();
        if generation != self.generation {
            self.generation = generation;
            self.tuner.set_params(&self.controls.params());
        }
        if self.probe.is_none() {
            self.probe = self.probe_rx.pop().ok();
        }
        for chunk in mono.chunks(MAX_BLOCK) {
            self.process_chunk(chunk);
        }
        self.meters.write(self.tuner.meters());
        let us = started.elapsed().as_micros().min(u128::from(u32::MAX)) as u32;
        self.stats.record(us);
    }

    fn process_chunk(&mut self, input: &[f32]) {
        let n = input.len();
        let out = &mut self.out[..n];
        let cable_out = &mut self.cable_out[..n];
        let target_gain = if let Some(probe) = self.probe.as_mut() {
            // Latency test: DSP bypassed, chirp to the monitor at unity.
            probe.step(input, out);
            cable_out.copy_from_slice(out);
            1.0
        } else {
            // Effects can be routed to the headphones, the virtual mic or both (FR-26).
            self.tuner.process_split(input, out, cable_out);
            self.controls.monitor_gain()
        };
        if let Some(probe) = self.probe.take_if(|p| p.done()) {
            if let Err(rtrb::PushError::Full(p)) = self.probe_done_tx.push(probe) {
                self.probe = Some(p);
            }
        }

        if let Some(cable) = self.cable_tx.as_mut() {
            push_all(cable, cable_out, &self.stats);
        }
        for s in out.iter_mut() {
            self.monitor_gain += (target_gain - self.monitor_gain) * self.gain_k;
            *s *= self.monitor_gain;
        }
        push_all(&mut self.monitor_tx, out, &self.stats);
    }
}

fn push_all(tx: &mut Producer<f32>, data: &[f32], stats: &CallbackStats) {
    let n = data.len().min(tx.slots());
    if n < data.len() {
        stats.ring_overflows.fetch_add(1, Ordering::Relaxed);
    }
    if n == 0 {
        return;
    }
    if let Ok(mut chunk) = tx.write_chunk(n) {
        let (a, b) = chunk.as_mut_slices();
        let split = a.len();
        a.copy_from_slice(&data[..split]);
        b.copy_from_slice(&data[split..n]);
        chunk.commit_all();
    }
}

/// Capture callback. The processor arrives through a one-slot ring once the
/// control plane knows the negotiated sample rate.
struct CaptureSide {
    slot: Consumer<Box<Processor>>,
    proc: Option<Box<Processor>>,
}

impl CaptureCallback for CaptureSide {
    fn on_capture(&mut self, mono: &[f32], info: &CallbackInfo) {
        if self.proc.is_none() {
            self.proc = self.slot.pop().ok();
        }
        if let Some(p) = self.proc.as_mut() {
            #[cfg(debug_assertions)]
            assert_no_alloc::assert_no_alloc(|| p.run(mono, info));
            #[cfg(not(debug_assertions))]
            p.run(mono, info);
        }
    }
}

// ───────────────────────────── render side ──────────────────────────────

#[derive(Clone, Copy)]
enum RingKind {
    Monitor,
    Cable,
}

struct RenderSide {
    slot: Consumer<Box<RingReader>>,
    reader: Option<Box<RingReader>>,
    stats: Arc<CallbackStats>,
    kind: RingKind,
}

impl RenderSide {
    fn new(slot: Consumer<Box<RingReader>>, stats: Arc<CallbackStats>, kind: RingKind) -> Self {
        Self {
            slot,
            reader: None,
            stats,
            kind,
        }
    }
}

impl RenderCallback for RenderSide {
    fn on_render(&mut self, out: &mut [f32], _info: &CallbackInfo) {
        if self.reader.is_none() {
            self.reader = self.slot.pop().ok();
        }
        let Some(r) = self.reader.as_mut() else {
            out.fill(0.0);
            return;
        };
        let report = r.read(out);
        let fill_us = (r.fill_ms() * 1000.0) as u32;
        let (underruns, fill) = match self.kind {
            RingKind::Monitor => (&self.stats.monitor_underruns, &self.stats.monitor_fill_us),
            RingKind::Cable => {
                let ppm = (r.relative_ratio() * 1e6) as u32;
                self.stats.cable_ratio_ppm.store(ppm, Ordering::Relaxed);
                (&self.stats.cable_underruns, &self.stats.cable_fill_us)
            }
        };
        if report.underrun {
            underruns.fetch_add(1, Ordering::Relaxed);
        }
        fill.store(fill_us, Ordering::Relaxed);
    }
}

// ─────────────────────────────── engine ────────────────────────────────

/// A running engine. Dropping it stops all streams (capture first).
pub struct Engine {
    capture: Option<StreamHandle>,
    monitor: Option<StreamHandle>,
    cable: Option<StreamHandle>,
    status: EngineStatus,
    meters: triple_buffer::Output<DspMeters>,
    stats: Arc<CallbackStats>,
    probe_tx: Producer<Box<LatencyProbe>>,
    probe_done_rx: Consumer<Box<LatencyProbe>>,
    sample_rate: u32,
    config: EngineConfig,
}

impl core::fmt::Debug for Engine {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Engine")
            .field("status", &self.status)
            .finish_non_exhaustive()
    }
}

fn same_clock(devices: &[DeviceInfo], a: &StreamInfo, b: &StreamInfo) -> bool {
    if a.sample_rate != b.sample_rate {
        return false;
    }
    let container = |id: &str| {
        devices
            .iter()
            .find(|d| d.id == id)
            .and_then(|d| d.container_id.clone())
    };
    matches!((container(&a.device_id), container(&b.device_id)), (Some(x), Some(y)) if x == y)
}

fn frames_at(frames: u32, from_rate: u32, to_rate: u32) -> usize {
    (u64::from(frames) * u64::from(to_rate) / u64::from(from_rate.max(1))) as usize
}

impl Engine {
    /// Open capture (fallback chain), monitor and optional cable streams and
    /// start processing (NFR-07: must produce audio in < 1 s).
    pub fn start(
        backend: &dyn AudioBackend,
        config: &EngineConfig,
        controls: Arc<SharedControls>,
    ) -> Result<Self, EngineError> {
        let devices = backend.list_devices().unwrap_or_default();
        let stats = Arc::new(CallbackStats::default());

        let (mut install_tx, install_rx) = RingBuffer::<Box<Processor>>::new(1);
        let capture = backend.open_capture(
            &CaptureRequest {
                device_id: config.capture_device.clone(),
                preferred_rate: config.preferred_rate,
                allow_exclusive: config.allow_exclusive,
                allow_asio: config.allow_asio,
                priority: ThreadPriority::Critical,
            },
            Box::new(CaptureSide {
                slot: install_rx,
                proc: None,
            }),
        )?;
        let cap = capture.info().clone();
        let sr = cap.sample_rate;

        let mut tuner = Tuner::new(TunerConfig {
            sample_rate: sr as f32,
            seed: config.seed,
        })?;
        let params = controls.params();
        tuner.set_params(&params);
        let dsp_latency_ms =
            params.voice_range.latency_samples(sr as f32) as f32 * 1000.0 / sr as f32;

        // Monitor path (FR-03).
        let ring_cap = (sr as usize).max(8_192);
        let (mon_tx, mon_rx) = RingBuffer::<f32>::new(ring_cap);
        let (mut mon_slot_tx, mon_slot_rx) = RingBuffer::<Box<RingReader>>::new(1);
        let monitor = backend.open_render(
            &RenderRequest {
                device_id: config.monitor_device.clone(),
                low_latency: true,
                priority: ThreadPriority::Critical,
            },
            Box::new(RenderSide::new(
                mon_slot_rx,
                stats.clone(),
                RingKind::Monitor,
            )),
        )?;
        let mon = monitor.info().clone();
        let mon_period_in = frames_at(mon.period_frames, mon.sample_rate, sr);
        let resampling = !same_clock(&devices, &cap, &mon);
        let (reader, ring_target) = if resampling {
            let target = cap.period_frames as usize + mon_period_in + 32;
            (
                RingReader::resampled(mon_rx, sr, mon.sample_rate, target)?,
                target,
            )
        } else {
            let target = (cap.period_frames as usize).max(mon_period_in);
            (RingReader::direct(mon_rx, sr, target), target)
        };
        let _ = mon_slot_tx.push(Box::new(reader));

        // Virtual mic path (FR-04).
        let mut cable_error = None;
        let mut cable = None;
        let mut cable_tx = None;
        if let Some(id) = &config.cable_device {
            let (tx, rx) = RingBuffer::<f32>::new(ring_cap);
            let (mut slot_tx, slot_rx) = RingBuffer::<Box<RingReader>>::new(1);
            match backend.open_render(
                &RenderRequest {
                    device_id: Some(id.clone()),
                    low_latency: false,
                    priority: ThreadPriority::High,
                },
                Box::new(RenderSide::new(slot_rx, stats.clone(), RingKind::Cable)),
            ) {
                Ok(h) => {
                    let ci = h.info().clone();
                    let period_in = frames_at(ci.period_frames, ci.sample_rate, sr);
                    let target = (2 * period_in).max(cap.period_frames as usize + period_in);
                    match RingReader::resampled(rx, sr, ci.sample_rate, target) {
                        Ok(r) => {
                            let _ = slot_tx.push(Box::new(r));
                            cable = Some(h);
                            cable_tx = Some(tx);
                        }
                        Err(e) => cable_error = Some(e.to_string()),
                    }
                }
                Err(e) => cable_error = Some(e.to_string()),
            }
        }

        let (meters_in, meters_out) = triple_buffer::TripleBuffer::new(&tuner.meters()).split();
        let (probe_tx, probe_rx) = RingBuffer::<Box<LatencyProbe>>::new(1);
        let (probe_done_tx, probe_done_rx) = RingBuffer::<Box<LatencyProbe>>::new(1);
        let processor = Box::new(Processor {
            tuner,
            generation: controls.generation(),
            controls: controls.clone(),
            out: vec![0.0; MAX_BLOCK],
            cable_out: vec![0.0; MAX_BLOCK],
            monitor_tx: mon_tx,
            cable_tx,
            meters: meters_in,
            stats: stats.clone(),
            probe_rx,
            probe_done_tx,
            probe: None,
            monitor_gain: 0.0,
            gain_k: 1.0 - (-1.0 / (MONITOR_GAIN_TAU_S * sr as f32)).exp(),
        });
        let _ = install_tx.push(processor);

        let ms = |frames: usize| frames as f32 * 1000.0 / sr as f32;
        let estimated_latency_ms = cap.period_ms()
            + cap.stream_latency_ms()
            + dsp_latency_ms
            + ms(ring_target)
            + mon.period_ms()
            + mon.stream_latency_ms();
        let status = EngineStatus {
            cable: cable.as_ref().map(|c: &StreamHandle| c.info().clone()),
            cable_error,
            capture: cap,
            monitor: mon,
            monitor_resampling: resampling,
            dsp_latency_ms,
            estimated_latency_ms,
        };
        Ok(Self {
            capture: Some(capture),
            monitor: Some(monitor),
            cable,
            status,
            meters: meters_out,
            stats,
            probe_tx,
            probe_done_rx,
            sample_rate: sr,
            config: config.clone(),
        })
    }

    pub fn status(&self) -> &EngineStatus {
        &self.status
    }

    pub fn config(&self) -> &EngineConfig {
        &self.config
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn stats(&self) -> &Arc<CallbackStats> {
        &self.stats
    }

    /// Latest meters and counters (poll at ~30 Hz).
    pub fn meters(&mut self) -> MeterSnapshot {
        let s = &self.stats;
        let c = |a: &std::sync::atomic::AtomicU64| {
            a.load(Ordering::Relaxed).min(u64::from(u32::MAX)) as u32
        };
        MeterSnapshot {
            dsp: *self.meters.read(),
            monitor_fill_ms: s.monitor_fill_us.load(Ordering::Relaxed) as f32 / 1000.0,
            cable_fill_ms: s.cable_fill_us.load(Ordering::Relaxed) as f32 / 1000.0,
            cable_ratio: s.cable_ratio_ppm.load(Ordering::Relaxed) as f32 / 1e6,
            xruns: s.xruns().min(u64::from(u32::MAX)) as u32,
            capture_discontinuities: c(&s.capture_discontinuities),
            monitor_underruns: c(&s.monitor_underruns),
            cable_underruns: c(&s.cable_underruns),
            ring_overflows: c(&s.ring_overflows),
            callback_p99_us: s.percentile_us(0.99),
            callback_max_us: s.max_us(),
            callback_budget_us: (self.status.capture.period_ms() * 1000.0) as u32,
        }
    }

    /// Check stream threads; the supervisor rebuilds on anything but Healthy.
    pub fn health(&self) -> EngineHealth {
        let check = |h: &Option<StreamHandle>, lost: EngineHealth| match h
            .as_ref()
            .map(|h| h.status().state())
        {
            Some(StreamState::DeviceLost) => Some(lost),
            Some(StreamState::Failed) => Some(EngineHealth::Failed(format!(
                "audio stream failed (0x{:08X})",
                h.as_ref().map(|h| h.status().error_code()).unwrap_or(0)
            ))),
            _ => None,
        };
        check(&self.capture, EngineHealth::CaptureLost)
            .or_else(|| check(&self.monitor, EngineHealth::MonitorLost))
            .or_else(|| check(&self.cable, EngineHealth::CableLost))
            .unwrap_or(EngineHealth::Healthy)
    }

    /// Run the acoustic loopback test (FR-16). Blocks the calling (control)
    /// thread for about one second.
    pub fn measure_latency(&mut self, timeout: Duration) -> Result<LatencyResult, EngineError> {
        while self.probe_done_rx.pop().is_ok() {}
        let probe = Box::new(LatencyProbe::new(self.sample_rate));
        let chirp = probe.chirp.clone();
        self.probe_tx.push(probe).map_err(|_| EngineError::Busy)?;
        let deadline = Instant::now() + timeout;
        loop {
            if let Ok(done) = self.probe_done_rx.pop() {
                let (lag, confidence) = analyze_loopback(&done.recorded, &chirp)
                    .ok_or(EngineError::NoLoopbackSignal)?;
                let hardware_ms = lag as f32 * 1000.0 / self.sample_rate as f32;
                return Ok(LatencyResult {
                    hardware_ms,
                    total_ms: hardware_ms + self.status.dsp_latency_ms,
                    confidence,
                });
            }
            if self.health() != EngineHealth::Healthy {
                return Err(EngineError::NotRunning);
            }
            if Instant::now() > deadline {
                return Err(EngineError::LatencyTimeout);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Stop capture first so producers go quiet, then the render streams.
    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        if let Some(c) = self.capture.take() {
            c.stop();
        }
        if let Some(m) = self.monitor.take() {
            m.stop();
        }
        if let Some(c) = self.cable.take() {
            c.stop();
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.shutdown();
    }
}
