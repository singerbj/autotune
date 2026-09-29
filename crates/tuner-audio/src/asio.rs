//! ASIO capture through cpal's ASIO host (feature `asio`, FR-02 tier 1).
//!
//! cpal streams are `!Send`, so each stream lives on a keeper thread that
//! builds it, reports the negotiated format, and parks until stopped. The
//! driver invokes our data callback on its own thread; that callback follows
//! the real-time rules (the engine callback arrives through a lock-free
//! single-slot ring after the stream is known to be running).
//!
//! cpal does not expose `ASIOControlPanel`, so [`AsioHost::open_control_panel`]
//! calls the driver's `IASIO::controlPanel` directly (ADR 0004).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::sync_channel;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, SampleFormat, StreamConfig};

use crate::backend::{
    CallbackInfo, CaptureCallback, CaptureRequest, StreamHandle, StreamState, StreamStatus,
};
use crate::types::{AudioError, BackendTier, DeviceInfo, Direction, StreamInfo};

/// Frames requested from the driver (Architecture › Capture step 1).
const ASIO_PERIOD: u32 = 64;
const MAX_FRAMES: usize = 8192;

/// Name of the driver whose buffers are currently open (for the panel).
static ACTIVE_DRIVER: Mutex<Option<String>> = Mutex::new(None);

#[derive(Debug, Default, Clone, Copy)]
pub struct AsioHost;

type Rejected = (AudioError, Box<dyn CaptureCallback>);

fn host() -> Result<cpal::Host, AudioError> {
    cpal::host_from_id(cpal::HostId::Asio)
        .map_err(|e| AudioError::TierUnavailable(leak(e.to_string())))
}

/// ASIO errors are rare and few; leaking their text keeps `AudioError`
/// `'static` without allocating on any audio path.
fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

impl AsioHost {
    /// `None` when no ASIO drivers are installed.
    pub fn new() -> Option<Self> {
        let h = host().ok()?;
        let any = h.input_devices().ok()?.next().is_some();
        any.then_some(Self)
    }

    fn driver_names() -> Vec<String> {
        host()
            .ok()
            .and_then(|h| h.input_devices().ok())
            .map(|it| it.filter_map(|d| d.name().ok()).collect())
            .unwrap_or_default()
    }

    /// Flag WASAPI endpoints that belong to an installed ASIO driver, by
    /// matching the driver's vendor word against the endpoint name
    /// (e.g. "Focusrite USB ASIO" ↔ "Analogue 1 + 2 (Focusrite USB Audio)").
    pub fn tag_devices(&self, devices: &mut [DeviceInfo]) {
        let drivers = Self::driver_names();
        for d in devices
            .iter_mut()
            .filter(|d| d.direction == Direction::Capture && !d.is_vb_cable)
        {
            let name = d.name.to_ascii_lowercase();
            d.asio_driver = drivers
                .iter()
                .find(|drv| {
                    drv.split_whitespace()
                        .next()
                        .map(|w| {
                            w.len() >= 3
                                && !w.eq_ignore_ascii_case("asio")
                                && name.contains(&w.to_ascii_lowercase())
                        })
                        .unwrap_or(false)
                })
                .cloned();
        }
    }

    /// Try to open `req`'s device through its ASIO driver. On failure the
    /// callback is handed back so the caller can fall back to WASAPI.
    pub fn open_capture_for(
        &self,
        devices: &[DeviceInfo],
        req: &CaptureRequest,
        cb: Box<dyn CaptureCallback>,
    ) -> Result<StreamHandle, Rejected> {
        let target = devices.iter().find(|d| match &req.device_id {
            Some(id) => &d.id == id,
            None => d.direction == Direction::Capture && d.is_default,
        });
        let Some((driver, dev_info)) =
            target.and_then(|d| d.asio_driver.clone().map(|a| (a, d.clone())))
        else {
            return Err((AudioError::TierUnavailable("device has no ASIO driver"), cb));
        };
        let status = Arc::new(StreamStatus::default());
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = sync_channel::<Result<StreamInfo, Rejected>>(1);
        let (st, sp) = (status.clone(), stop.clone());
        let preferred = req.preferred_rate;
        let thread = std::thread::Builder::new()
            .name("tuner-asio".into())
            .spawn(move || keeper(driver, dev_info, preferred, cb, tx, st, sp))
            .map_err(|e| AudioError::Thread(e.to_string()));
        let thread = match thread {
            Ok(t) => t,
            Err(e) => return Err((e, Box::new(|_: &[f32], _: &CallbackInfo| {}))),
        };
        match rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(info)) => Ok(StreamHandle::new(info, status, stop, thread)),
            Ok(Err(rejected)) => {
                let _ = thread.join();
                Err(rejected)
            }
            Err(_) => {
                stop.store(true, Ordering::Release);
                let _ = thread.join();
                Err((
                    AudioError::Thread("ASIO driver did not start in time".into()),
                    Box::new(|_: &[f32], _: &CallbackInfo| {}),
                ))
            }
        }
    }

    /// Open the control panel of the driver in use (or the first driver).
    pub fn open_control_panel(&self) -> Result<(), AudioError> {
        let name = ACTIVE_DRIVER
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .or_else(|| Self::driver_names().into_iter().next())
            .ok_or(AudioError::TierUnavailable("no ASIO driver installed"))?;
        panel::open(&name)
    }
}

fn keeper(
    driver: String,
    dev_info: DeviceInfo,
    preferred_rate: u32,
    cb: Box<dyn CaptureCallback>,
    tx: std::sync::mpsc::SyncSender<Result<StreamInfo, Rejected>>,
    status: Arc<StreamStatus>,
    stop: Arc<AtomicBool>,
) {
    let fail = |e: AudioError, cb| {
        let _ = tx.send(Err((e, cb)));
    };
    let host = match host() {
        Ok(h) => h,
        Err(e) => return fail(e, cb),
    };
    let device = match host
        .input_devices()
        .ok()
        .and_then(|mut it| it.find(|d| d.name().ok().as_deref() == Some(driver.as_str())))
    {
        Some(d) => d,
        None => return fail(AudioError::DeviceNotFound(driver), cb),
    };
    let default = match device.default_input_config() {
        Ok(c) => c,
        Err(e) => return fail(AudioError::TierUnavailable(leak(e.to_string())), cb),
    };
    let format = default.sample_format();
    let channels = default.channels();
    let rate = if device
        .supported_input_configs()
        .map(|mut it| {
            it.any(|r| {
                r.min_sample_rate().0 <= preferred_rate && r.max_sample_rate().0 >= preferred_rate
            })
        })
        .unwrap_or(false)
    {
        preferred_rate
    } else {
        default.sample_rate().0
    };

    // Ask for 64 frames when the driver allows it, else its closest size.
    let buffer = match default.buffer_size() {
        cpal::SupportedBufferSize::Range { min, max } => {
            BufferSize::Fixed(ASIO_PERIOD.clamp(*min, (*max).max(*min)))
        }
        cpal::SupportedBufferSize::Unknown => BufferSize::Default,
    };
    let period = match buffer {
        BufferSize::Fixed(n) => n,
        BufferSize::Default => ASIO_PERIOD,
    };

    let (mut cb_tx, mut cb_rx) = rtrb::RingBuffer::<Box<dyn CaptureCallback>>::new(1);
    let build = |buffer: BufferSize, st: Arc<StreamStatus>| {
        let config = StreamConfig {
            channels,
            sample_rate: cpal::SampleRate(rate),
            buffer_size: buffer,
        };
        let mut engine_cb: Option<Box<dyn CaptureCallback>> = None;
        let mut mono = vec![0.0f32; MAX_FRAMES];
        let ch = usize::from(channels.max(1));
        let info = CallbackInfo {
            sample_rate: rate,
            discontinuity: false,
        };
        let st_err = st.clone();
        device.build_input_stream_raw(
            &config,
            format,
            move |data: &cpal::Data, _: &cpal::InputCallbackInfo| {
                if engine_cb.is_none() {
                    engine_cb = cb_rx.pop().ok();
                }
                let frames = (data.len() / ch).min(MAX_FRAMES);
                match format {
                    SampleFormat::F32 => {
                        if let Some(s) = data.as_slice::<f32>() {
                            for (i, m) in mono[..frames].iter_mut().enumerate() {
                                *m = s[i * ch];
                            }
                        }
                    }
                    SampleFormat::I32 => {
                        if let Some(s) = data.as_slice::<i32>() {
                            for (i, m) in mono[..frames].iter_mut().enumerate() {
                                *m = s[i * ch] as f32 / 2_147_483_648.0;
                            }
                        }
                    }
                    SampleFormat::I16 => {
                        if let Some(s) = data.as_slice::<i16>() {
                            for (i, m) in mono[..frames].iter_mut().enumerate() {
                                *m = f32::from(s[i * ch]) / 32_768.0;
                            }
                        }
                    }
                    _ => mono[..frames].fill(0.0),
                }
                if let Some(cb) = engine_cb.as_mut() {
                    cb.on_capture(&mono[..frames], &info);
                }
                st.tick();
            },
            move |err| {
                st_err.set_state(match err {
                    cpal::StreamError::DeviceNotAvailable => StreamState::DeviceLost,
                    _ => StreamState::Failed,
                });
            },
            None,
        )
    };
    let stream = match build(buffer, status.clone()) {
        Ok(s) => s,
        Err(e) => return fail(AudioError::TierUnavailable(leak(e.to_string())), cb),
    };
    if let Err(e) = stream.play() {
        return fail(AudioError::TierUnavailable(leak(e.to_string())), cb);
    }
    // Stream is live: hand the engine callback to the driver thread.
    if let Err(rtrb::PushError::Full(cb)) = cb_tx.push(cb) {
        return fail(AudioError::Thread("callback slot full".into()), cb);
    }
    *ACTIVE_DRIVER.lock().unwrap_or_else(PoisonError::into_inner) = Some(driver.clone());
    status.set_state(StreamState::Running);
    let _ = tx.send(Ok(StreamInfo {
        device_id: dev_info.id,
        device_name: format!("{} (ASIO: {driver})", dev_info.name),
        tier: BackendTier::Asio,
        sample_rate: rate,
        channels,
        period_frames: period,
        stream_latency_frames: 0,
        fallbacks: vec![],
    }));
    while !stop.load(Ordering::Acquire) && status.is_healthy() {
        std::thread::sleep(Duration::from_millis(20));
    }
    drop(stream);
    *ACTIVE_DRIVER.lock().unwrap_or_else(PoisonError::into_inner) = None;
}

mod panel {
    //! Direct `IASIO::controlPanel` call. ASIO drivers are in-process COM
    //! servers whose IID equals their CLSID, registered under
    //! `HKLM\SOFTWARE\ASIO\<driver>\CLSID`.

    use core::ffi::c_void;

    use windows::core::{IUnknown, Interface, GUID, HSTRING};
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::Com::{CLSIDFromString, CoCreateInstance, CLSCTX_INPROC_SERVER};
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};

    use crate::types::AudioError;
    use crate::wasapi::ComGuard;

    /// IUnknown (3) + init, then 17 methods, then controlPanel (index 21).
    #[repr(C)]
    struct IAsioVtbl {
        _unknown: [usize; 3],
        init: unsafe extern "system" fn(*mut c_void, *mut c_void) -> i32,
        _methods: [usize; 17],
        control_panel: unsafe extern "system" fn(*mut c_void) -> i32,
    }

    fn clsid(driver: &str) -> Result<GUID, AudioError> {
        let key = HSTRING::from(format!("SOFTWARE\\ASIO\\{driver}"));
        let mut buf = [0u16; 64];
        let mut size = (buf.len() * 2) as u32;
        // SAFETY: valid key/value strings and a buffer of `size` bytes.
        let rc = unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                &key,
                &HSTRING::from("CLSID"),
                RRF_RT_REG_SZ,
                None,
                Some(buf.as_mut_ptr().cast()),
                Some(&mut size),
            )
        };
        if rc != ERROR_SUCCESS {
            return Err(AudioError::TierUnavailable("ASIO driver is not registered"));
        }
        // SAFETY: RegGetValueW null-terminated the string inside `buf`.
        unsafe { CLSIDFromString(windows::core::PCWSTR(buf.as_ptr())) }
            .map_err(|_| AudioError::TierUnavailable("ASIO driver CLSID is invalid"))
    }

    pub(super) fn open(driver: &str) -> Result<(), AudioError> {
        let _com = ComGuard::init();
        let id = clsid(driver)?;
        // SAFETY: COM initialised; `id` identifies an in-proc server.
        let unk: IUnknown = unsafe { CoCreateInstance(&id, None, CLSCTX_INPROC_SERVER) }
            .map_err(|_| AudioError::TierUnavailable("could not load the ASIO driver"))?;
        let mut raw: *mut c_void = core::ptr::null_mut();
        // SAFETY: querying the driver's own interface (IID == CLSID).
        unsafe { unk.query(&id, &mut raw) }
            .ok()
            .map_err(|_| AudioError::TierUnavailable("driver does not implement IASIO"))?;
        // SAFETY: `raw` is a live IASIO pointer whose first field is its vtable
        // (x64 has a single calling convention, so thiscall == system); we
        // release the reference we took exactly once.
        unsafe {
            let vtbl = *(raw as *const *const IAsioVtbl);
            let ok = ((*vtbl).init)(raw, core::ptr::null_mut());
            let rc = if ok != 0 {
                ((*vtbl).control_panel)(raw)
            } else {
                -1
            };
            IUnknown::from_raw(raw);
            if rc != 0 {
                return Err(AudioError::TierUnavailable(
                    "driver refused to open its control panel",
                ));
            }
        }
        Ok(())
    }
}
