//! Stream threads. Each stream owns one thread that initialises COM, opens
//! its IAudioClient, joins MMCSS "Pro Audio", and then waits only on its own
//! WASAPI event (hard rule). The opener waits for the negotiated
//! [`StreamInfo`] over a rendezvous channel.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, RecvTimeoutError, SyncSender};
use std::sync::Arc;
use std::time::Duration;

use windows::core::{w, Interface};
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
use windows::Win32::Media::Audio::{
    IAudioCaptureClient, IAudioClient, IAudioClient3, IAudioRenderClient, IMMDevice,
    AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY, AUDCLNT_BUFFERFLAGS_SILENT,
    AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED, AUDCLNT_E_DEVICE_INVALIDATED, AUDCLNT_SHAREMODE_EXCLUSIVE,
    AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
};
use windows::Win32::System::Com::{CoTaskMemFree, CLSCTX_ALL};
use windows::Win32::System::Threading::{
    AvRevertMmThreadCharacteristics, AvSetMmThreadCharacteristicsW, AvSetMmThreadPriority,
    CreateEventW, WaitForSingleObject, AVRT_PRIORITY_CRITICAL, AVRT_PRIORITY_HIGH,
};

use super::com::{map_err, ComGuard};
use super::enumerate::{device_id, device_name, enumerator, get_device};
use super::format::{from_mono, to_mono, Format};
use crate::backend::{
    open_with_fallback, CallbackInfo, CaptureCallback, CaptureRequest, RenderCallback,
    RenderRequest, StreamHandle, StreamState, StreamStatus, ThreadPriority,
};
use crate::types::{AudioError, BackendTier, Direction, StreamInfo};

const HNS_PER_SEC: f64 = 10_000_000.0;
const OPEN_TIMEOUT: Duration = Duration::from_secs(5);
/// Wake at least this often to notice a stop request.
const WAIT_MS: u32 = 200;

struct OwnedEvent(HANDLE);

impl Drop for OwnedEvent {
    fn drop(&mut self) {
        // SAFETY: the handle was created by CreateEventW and is closed once.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

struct Mmcss(HANDLE);

impl Mmcss {
    fn join(p: ThreadPriority) -> Option<Self> {
        let mut index = 0u32;
        // SAFETY: valid task name literal and out pointer.
        let h = unsafe { AvSetMmThreadCharacteristicsW(w!("Pro Audio"), &mut index) }.ok()?;
        let prio = match p {
            ThreadPriority::Critical => AVRT_PRIORITY_CRITICAL,
            ThreadPriority::High => AVRT_PRIORITY_HIGH,
        };
        // SAFETY: `h` is a valid MMCSS handle.
        let _ = unsafe { AvSetMmThreadPriority(h, prio) };
        Some(Self(h))
    }
}

impl Drop for Mmcss {
    fn drop(&mut self) {
        // SAFETY: `self.0` came from AvSetMmThreadCharacteristicsW on this thread.
        let _ = unsafe { AvRevertMmThreadCharacteristics(self.0) };
    }
}

/// An initialised (not yet started) client.
struct Opened {
    client: IAudioClient,
    format: Format,
    buffer_frames: u32,
    exclusive: bool,
    info: StreamInfo,
}

fn activate(dev: &IMMDevice) -> Result<IAudioClient, AudioError> {
    // SAFETY: valid endpoint; COM initialised on this thread.
    unsafe { dev.Activate::<IAudioClient>(CLSCTX_ALL, None) }.map_err(|e| map_err("Activate", &e))
}

fn mix_format(client: &IAudioClient) -> Result<Format, AudioError> {
    // SAFETY: GetMixFormat returns a CoTaskMem-allocated WAVEFORMATEX that we
    // parse (copying it) and then free exactly once.
    unsafe {
        let p = client
            .GetMixFormat()
            .map_err(|e| map_err("GetMixFormat", &e))?;
        let f = Format::from_ptr(p);
        CoTaskMemFree(Some(p as *const core::ffi::c_void));
        f.ok_or(AudioError::FormatNotSupported)
    }
}

fn hns_to_frames(hns: i64, rate: u32) -> u32 {
    (hns as f64 * f64::from(rate) / HNS_PER_SEC).round() as u32
}

/// Initialise `dev` for `tier`. Returns the client with its event set.
fn init_tier(
    dev: &IMMDevice,
    tier: BackendTier,
    dir: Direction,
    preferred_rate: u32,
) -> Result<(IAudioClient, Format, u32, bool), AudioError> {
    let mut client = activate(dev)?;
    let mix = mix_format(&client)?;
    let flags = AUDCLNT_STREAMFLAGS_EVENTCALLBACK;
    match tier {
        BackendTier::WasapiExclusive => {
            let (mut def, mut min) = (0i64, 0i64);
            // SAFETY: valid out pointers.
            unsafe { client.GetDevicePeriod(Some(&mut def), Some(&mut min)) }
                .map_err(|e| map_err("GetDevicePeriod", &e))?;
            let fmt = Format::exclusive_candidates(preferred_rate, &mix)
                .into_iter()
                .find(|f| {
                    // SAFETY: `f.as_ptr()` points to a live format.
                    unsafe {
                        client.IsFormatSupported(AUDCLNT_SHAREMODE_EXCLUSIVE, f.as_ptr(), None)
                    }
                    .is_ok()
                })
                .ok_or(AudioError::FormatNotSupported)?;
            // SAFETY: `fmt` outlives the call.
            let r = unsafe {
                client.Initialize(
                    AUDCLNT_SHAREMODE_EXCLUSIVE,
                    flags,
                    min,
                    min,
                    fmt.as_ptr(),
                    None,
                )
            };
            match r {
                Err(e) if e.code() == AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED => {
                    // Align the period to the buffer the driver wants and retry
                    // on a fresh client (documented WASAPI procedure).
                    // SAFETY: valid client.
                    let frames = unsafe { client.GetBufferSize() }
                        .map_err(|e| map_err("GetBufferSize", &e))?;
                    let period =
                        (HNS_PER_SEC * f64::from(frames) / f64::from(fmt.rate)).round() as i64;
                    client = activate(dev)?;
                    // SAFETY: `fmt` outlives the call.
                    unsafe {
                        client.Initialize(
                            AUDCLNT_SHAREMODE_EXCLUSIVE,
                            flags,
                            period,
                            period,
                            fmt.as_ptr(),
                            None,
                        )
                    }
                    .map_err(|e| map_err("Initialize(exclusive, aligned)", &e))?;
                }
                other => other.map_err(|e| map_err("Initialize(exclusive)", &e))?,
            }
            // SAFETY: initialised client.
            let frames =
                unsafe { client.GetBufferSize() }.map_err(|e| map_err("GetBufferSize", &e))?;
            Ok((client, fmt, frames, true))
        }
        BackendTier::WasapiSharedLowLatency => {
            let c3: IAudioClient3 = client
                .cast()
                .map_err(|_| AudioError::TierUnavailable("IAudioClient3 not available"))?;
            let (mut def, mut fun, mut min, mut max) = (0u32, 0u32, 0u32, 0u32);
            // SAFETY: valid format pointer and out pointers.
            unsafe {
                c3.GetSharedModeEnginePeriod(mix.as_ptr(), &mut def, &mut fun, &mut min, &mut max)
            }
            .map_err(|e| map_err("GetSharedModeEnginePeriod", &e))?;
            if min == 0 || min >= def {
                return Err(AudioError::TierUnavailable(
                    "driver offers no low-latency period",
                ));
            }
            // SAFETY: valid format pointer.
            unsafe { c3.InitializeSharedAudioStream(flags, min, mix.as_ptr(), None) }
                .map_err(|e| map_err("InitializeSharedAudioStream", &e))?;
            Ok((client, mix, min, false))
        }
        BackendTier::WasapiShared => {
            // SAFETY: valid format pointer.
            unsafe { client.Initialize(AUDCLNT_SHAREMODE_SHARED, flags, 0, 0, mix.as_ptr(), None) }
                .map_err(|e| map_err("Initialize(shared)", &e))?;
            let mut def = 0i64;
            // SAFETY: valid out pointer.
            unsafe { client.GetDevicePeriod(Some(&mut def), None) }
                .map_err(|e| map_err("GetDevicePeriod", &e))?;
            let _ = dir;
            Ok((client, mix, hns_to_frames(def, mix.rate).max(1), false))
        }
        BackendTier::Asio | BackendTier::Mock => {
            Err(AudioError::TierUnavailable("not a WASAPI tier"))
        }
    }
}

fn open(
    dir: Direction,
    device: Option<&str>,
    tiers: &[BackendTier],
    preferred_rate: u32,
) -> Result<Opened, AudioError> {
    let en = enumerator()?;
    let dev = get_device(&en, device, dir)?;
    let id = device_id(&dev)?;
    let name = device_name(&dev);
    let ((client, format, period, exclusive), tier, fallbacks) =
        open_with_fallback(tiers, |t| init_tier(&dev, t, dir, preferred_rate))?;
    // SAFETY: initialised client.
    let buffer_frames =
        unsafe { client.GetBufferSize() }.map_err(|e| map_err("GetBufferSize", &e))?;
    // SAFETY: initialised client.
    let latency = unsafe { client.GetStreamLatency() }.unwrap_or(0);
    let info = StreamInfo {
        device_id: id,
        device_name: name,
        tier,
        sample_rate: format.rate,
        channels: format.channels,
        period_frames: if exclusive { buffer_frames } else { period },
        stream_latency_frames: hns_to_frames(latency, format.rate),
        fallbacks,
    };
    Ok(Opened {
        client,
        format,
        buffer_frames,
        exclusive,
        info,
    })
}

enum Job {
    Capture(CaptureRequest, Box<dyn CaptureCallback>),
    Render(RenderRequest, Box<dyn RenderCallback>),
}

pub(crate) fn spawn_capture(
    req: CaptureRequest,
    cb: Box<dyn CaptureCallback>,
) -> Result<StreamHandle, AudioError> {
    spawn(Job::Capture(req, cb), "tuner-capture")
}

pub(crate) fn spawn_render(
    req: RenderRequest,
    cb: Box<dyn RenderCallback>,
) -> Result<StreamHandle, AudioError> {
    spawn(Job::Render(req, cb), "tuner-render")
}

fn spawn(job: Job, name: &str) -> Result<StreamHandle, AudioError> {
    let (tx, rx) = sync_channel::<Result<StreamInfo, AudioError>>(1);
    let status = Arc::new(StreamStatus::default());
    let stop = Arc::new(AtomicBool::new(false));
    let (st, sp) = (status.clone(), stop.clone());
    let thread = std::thread::Builder::new()
        .name(name.into())
        .spawn(move || thread_main(job, tx, &st, &sp))
        .map_err(|e| AudioError::Thread(e.to_string()))?;
    match rx.recv_timeout(OPEN_TIMEOUT) {
        Ok(Ok(info)) => Ok(StreamHandle::new(info, status, stop, thread)),
        Ok(Err(e)) => {
            let _ = thread.join();
            Err(e)
        }
        Err(RecvTimeoutError::Timeout) | Err(RecvTimeoutError::Disconnected) => {
            stop.store(true, Ordering::Release);
            let _ = thread.join();
            Err(AudioError::Thread(
                "audio device did not open in time".into(),
            ))
        }
    }
}

fn thread_main(
    job: Job,
    tx: SyncSender<Result<StreamInfo, AudioError>>,
    status: &StreamStatus,
    stop: &AtomicBool,
) {
    let _com = ComGuard::init();
    let (dir, device, tiers, rate, priority): (_, _, Vec<BackendTier>, _, _) = match &job {
        Job::Capture(r, _) => {
            let mut t = Vec::new();
            if r.allow_exclusive {
                t.push(BackendTier::WasapiExclusive);
            }
            t.push(BackendTier::WasapiSharedLowLatency);
            t.push(BackendTier::WasapiShared);
            (
                Direction::Capture,
                r.device_id.clone(),
                t,
                r.preferred_rate,
                r.priority,
            )
        }
        Job::Render(r, _) => {
            let mut t = Vec::new();
            if r.low_latency {
                t.push(BackendTier::WasapiSharedLowLatency);
            }
            t.push(BackendTier::WasapiShared);
            (
                Direction::Render,
                r.device_id.clone(),
                t,
                48_000,
                r.priority,
            )
        }
    };
    let opened = match open(dir, device.as_deref(), &tiers, rate) {
        Ok(o) => o,
        Err(e) => {
            let _ = tx.send(Err(e));
            return;
        }
    };
    // SAFETY: CreateEventW with no attributes/name has no preconditions.
    let event = match unsafe { CreateEventW(None, false, false, None) } {
        Ok(h) => OwnedEvent(h),
        Err(e) => {
            let _ = tx.send(Err(map_err("CreateEventW", &e)));
            return;
        }
    };
    // SAFETY: client initialised with EVENTCALLBACK; the event outlives the stream.
    if let Err(e) = unsafe { opened.client.SetEventHandle(event.0) } {
        let _ = tx.send(Err(map_err("SetEventHandle", &e)));
        return;
    }
    let _mmcss = Mmcss::join(priority);
    let result = match job {
        Job::Capture(r, cb) => run_capture(&opened, &event, cb, r.priority, tx, status, stop),
        Job::Render(_, cb) => run_render(&opened, &event, cb, tx, status, stop),
    };
    // SAFETY: stopping an initialised client is always allowed.
    let _ = unsafe { opened.client.Stop() };
    match result {
        Ok(()) => status.set_state(StreamState::Stopped),
        Err(e) => {
            status.set_error_code(e.code().0 as u32);
            status.set_state(if e.code() == AUDCLNT_E_DEVICE_INVALIDATED {
                StreamState::DeviceLost
            } else {
                StreamState::Failed
            });
        }
    }
}

/// Wait for the stream event; `Ok(false)` means stop was requested.
fn wait(event: &OwnedEvent, stop: &AtomicBool) -> bool {
    loop {
        if stop.load(Ordering::Acquire) {
            return false;
        }
        // SAFETY: valid event handle. This is the only wait on the audio thread.
        if unsafe { WaitForSingleObject(event.0, WAIT_MS) } == WAIT_OBJECT_0 {
            return true;
        }
    }
}

fn run_capture(
    o: &Opened,
    event: &OwnedEvent,
    mut cb: Box<dyn CaptureCallback>,
    _prio: ThreadPriority,
    tx: SyncSender<Result<StreamInfo, AudioError>>,
    status: &StreamStatus,
    stop: &AtomicBool,
) -> windows::core::Result<()> {
    let capture: IAudioCaptureClient = {
        // SAFETY: initialised client.
        match unsafe { o.client.GetService::<IAudioCaptureClient>() } {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.send(Err(map_err("GetService(capture)", &e)));
                return Ok(());
            }
        }
    };
    // Worst case: a whole buffer in one packet.
    let mut mono = vec![0.0f32; o.buffer_frames as usize * 2];
    // SAFETY: initialised client.
    if let Err(e) = unsafe { o.client.Start() } {
        let _ = tx.send(Err(map_err("Start(capture)", &e)));
        return Ok(());
    }
    let _ = tx.send(Ok(o.info.clone()));
    status.set_state(StreamState::Running);
    let mut info = CallbackInfo {
        sample_rate: o.format.rate,
        discontinuity: false,
    };
    while wait(event, stop) {
        loop {
            // SAFETY: running capture client.
            let packet = unsafe { capture.GetNextPacketSize() }?;
            if packet == 0 {
                break;
            }
            let mut data: *mut u8 = core::ptr::null_mut();
            let (mut frames, mut flags) = (0u32, 0u32);
            // SAFETY: valid out pointers; the buffer is released below.
            unsafe { capture.GetBuffer(&mut data, &mut frames, &mut flags, None, None) }?;
            if frames == 0 {
                // SAFETY: releasing the (empty) buffer we just got.
                unsafe { capture.ReleaseBuffer(0) }?;
                break;
            }
            let n = (frames as usize).min(mono.len());
            if flags & (AUDCLNT_BUFFERFLAGS_SILENT.0 as u32) != 0 || data.is_null() {
                mono[..n].fill(0.0);
            } else {
                // SAFETY: WASAPI guarantees `frames` frames at `data`.
                unsafe { to_mono(data, n, &o.format, 0, &mut mono) };
            }
            info.discontinuity = flags & (AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY.0 as u32) != 0;
            if info.discontinuity {
                status.add_xrun();
            }
            // SAFETY: releasing exactly what GetBuffer returned.
            unsafe { capture.ReleaseBuffer(frames) }?;
            cb.on_capture(&mono[..n], &info);
            status.tick();
        }
    }
    Ok(())
}

fn run_render(
    o: &Opened,
    event: &OwnedEvent,
    mut cb: Box<dyn RenderCallback>,
    tx: SyncSender<Result<StreamInfo, AudioError>>,
    status: &StreamStatus,
    stop: &AtomicBool,
) -> windows::core::Result<()> {
    // SAFETY: initialised client.
    let render: IAudioRenderClient = match unsafe { o.client.GetService::<IAudioRenderClient>() } {
        Ok(r) => r,
        Err(e) => {
            let _ = tx.send(Err(map_err("GetService(render)", &e)));
            return Ok(());
        }
    };
    let total = o.buffer_frames;
    let mut mono = vec![0.0f32; total as usize];
    // Pre-roll silence so the first period doesn't glitch.
    // SAFETY: requesting the whole (empty) buffer before Start, releasing it silent.
    unsafe {
        if render.GetBuffer(total).is_ok() {
            let _ = render.ReleaseBuffer(total, AUDCLNT_BUFFERFLAGS_SILENT.0 as u32);
        }
    }
    // SAFETY: initialised client.
    if let Err(e) = unsafe { o.client.Start() } {
        let _ = tx.send(Err(map_err("Start(render)", &e)));
        return Ok(());
    }
    let _ = tx.send(Ok(o.info.clone()));
    status.set_state(StreamState::Running);
    let info = CallbackInfo {
        sample_rate: o.format.rate,
        discontinuity: false,
    };
    while wait(event, stop) {
        let padding = if o.exclusive {
            0
        } else {
            // SAFETY: running client.
            unsafe { o.client.GetCurrentPadding() }?
        };
        let avail = total.saturating_sub(padding);
        if avail == 0 {
            continue;
        }
        let n = avail as usize;
        cb.on_render(&mut mono[..n], &info);
        // SAFETY: `avail` ≤ free space; the pointer covers `avail` frames of
        // `format` and is released immediately after writing.
        unsafe {
            let data = render.GetBuffer(avail)?;
            from_mono(&mono[..n], data, &o.format);
            render.ReleaseBuffer(avail, 0)?;
        }
        status.tick();
    }
    Ok(())
}
