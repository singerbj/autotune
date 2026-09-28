//! WASAPI FFI smoke tests on real Windows (CI runners have no audio
//! endpoints, so these check graceful behaviour, not audio).

#![cfg(windows)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use tuner_audio::wasapi::WasapiBackend;
use tuner_audio::{AudioBackend, AudioError, CallbackInfo, CaptureRequest, RenderRequest};

#[test]
fn fr01_enumeration_succeeds_or_fails_cleanly() {
    let b = WasapiBackend::new();
    match b.list_devices() {
        Ok(devices) => {
            for d in &devices {
                assert!(!d.id.is_empty());
                assert!(!d.name.is_empty());
            }
            println!("{} endpoints", devices.len());
        }
        Err(e) => println!("enumeration unavailable: {e}"),
    }
}

#[test]
fn unknown_device_is_reported_not_panicking() {
    let b = WasapiBackend::new();
    let r = b.open_capture(
        &CaptureRequest {
            device_id: Some("{0.0.1.00000000}.{00000000-0000-0000-0000-000000000000}".into()),
            ..Default::default()
        },
        Box::new(|_: &[f32], _: &CallbackInfo| {}),
    );
    assert!(r.is_err());
}

struct Silence;
impl tuner_audio::RenderCallback for Silence {
    fn on_render(&mut self, out: &mut [f32], _: &CallbackInfo) {
        out.fill(0.0);
    }
}

#[test]
fn default_render_opens_or_reports_no_device() {
    let b = WasapiBackend::new();
    match b.open_render(&RenderRequest::default(), Box::new(Silence)) {
        Ok(h) => {
            assert!(h.info().sample_rate > 0);
            h.stop();
        }
        Err(AudioError::NoDevice) | Err(AudioError::DeviceNotFound(_)) => {}
        Err(e) => println!("render unavailable: {e}"),
    }
}
