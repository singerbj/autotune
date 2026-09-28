//! WASAPI backend (Windows): event-driven exclusive and shared streams,
//! `IAudioClient3` low-latency shared mode, and endpoint enumeration.

mod com;
mod enumerate;
mod format;
mod stream;

pub use com::ComGuard;

use crate::backend::{
    AudioBackend, CaptureCallback, CaptureRequest, RenderCallback, RenderRequest, StreamHandle,
};
use crate::types::{AudioError, DeviceInfo};

/// The default Windows backend. When built with the `asio` feature, capture
/// devices that belong to an installed ASIO driver try ASIO first (FR-02).
#[derive(Debug, Default)]
pub struct WasapiBackend {
    #[cfg(feature = "asio")]
    asio: Option<crate::asio::AsioHost>,
}

impl WasapiBackend {
    pub fn new() -> Self {
        Self {
            #[cfg(feature = "asio")]
            asio: crate::asio::AsioHost::new(),
        }
    }
}

impl AudioBackend for WasapiBackend {
    fn name(&self) -> &'static str {
        "wasapi"
    }

    fn list_devices(&self) -> Result<Vec<DeviceInfo>, AudioError> {
        let _com = ComGuard::init();
        #[allow(unused_mut)]
        let mut devices = enumerate::list_devices()?;
        #[cfg(feature = "asio")]
        if let Some(asio) = &self.asio {
            asio.tag_devices(&mut devices);
        }
        Ok(devices)
    }

    fn open_capture(
        &self,
        req: &CaptureRequest,
        cb: Box<dyn CaptureCallback>,
    ) -> Result<StreamHandle, AudioError> {
        #[cfg(feature = "asio")]
        let mut asio_attempt = None;
        #[cfg(feature = "asio")]
        let cb = match (&self.asio, req.allow_asio) {
            (Some(asio), true) => {
                let devices = self.list_devices()?;
                match asio.open_capture_for(&devices, req, cb) {
                    Ok(handle) => return Ok(handle),
                    Err((e, cb)) => {
                        asio_attempt = Some(crate::TierAttempt {
                            tier: crate::BackendTier::Asio,
                            error: e.to_string(),
                        });
                        cb
                    }
                }
            }
            _ => cb,
        };
        #[allow(unused_mut)]
        let mut handle = stream::spawn_capture(req.clone(), cb)?;
        #[cfg(feature = "asio")]
        if let Some(a) = asio_attempt {
            handle.prepend_fallback(a);
        }
        Ok(handle)
    }

    fn open_render(
        &self,
        req: &RenderRequest,
        cb: Box<dyn RenderCallback>,
    ) -> Result<StreamHandle, AudioError> {
        stream::spawn_render(req.clone(), cb)
    }

    fn open_asio_panel(&self) -> Result<(), AudioError> {
        #[cfg(feature = "asio")]
        if let Some(asio) = &self.asio {
            return asio.open_control_panel();
        }
        Err(AudioError::TierUnavailable(
            "ASIO support is not compiled in",
        ))
    }
}
