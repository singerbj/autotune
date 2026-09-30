//! Engine behaviour against the mock backend (runs on every OS in CI).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use tuner_audio::mock::{MockBackend, MockSignal};
use tuner_audio::{AudioBackend, BackendTier, DeviceInfo, Direction};
use tuner_dsp::TuningParams;
use tuner_engine::{
    Engine, EngineConfig, EngineHealth, SharedControls, Supervisor, SupervisorState,
};

fn wait_until(timeout: Duration, mut f: impl FnMut() -> bool) -> bool {
    let end = Instant::now() + timeout;
    while Instant::now() < end {
        if f() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    f()
}

fn headset_config() -> EngineConfig {
    EngineConfig {
        capture_device: Some("mic".into()),
        monitor_device: Some("hp".into()),
        cable_device: Some("cable-in".into()),
        ..Default::default()
    }
}

#[test]
fn nfr07_start_to_audible_under_one_second() {
    let b = MockBackend::with_default_devices();
    b.set_signal(MockSignal::Sine {
        hz: 220.0,
        amp: 0.3,
    });
    let t0 = Instant::now();
    let engine = Engine::start(&b, &headset_config(), Arc::new(SharedControls::default())).unwrap();
    assert!(wait_until(Duration::from_secs(2), || b
        .first_sound("hp")
        .is_some()));
    let elapsed = b.first_sound("hp").unwrap() - t0;
    assert!(elapsed < Duration::from_secs(1), "{elapsed:?}");
    engine.stop();
}

#[test]
fn fr02_fr03_fr04_routes_to_monitor_and_cable_and_reports_tier() {
    let b = MockBackend::with_default_devices();
    b.set_signal(MockSignal::Sine {
        hz: 220.0,
        amp: 0.3,
    });
    let mut engine =
        Engine::start(&b, &headset_config(), Arc::new(SharedControls::default())).unwrap();
    let st = engine.status().clone();
    assert_eq!(st.capture.tier, BackendTier::WasapiExclusive);
    assert!(!st.monitor_resampling, "headset mic+phones share a clock");
    assert!(st.cable.is_some());
    assert!(st.estimated_latency_ms > st.dsp_latency_ms);
    assert!(wait_until(Duration::from_secs(2), || {
        b.rendered("cable-in").iter().any(|v| v.abs() > 0.05)
    }));
    // FR-17: meters show the detected pitch.
    assert!(wait_until(Duration::from_secs(2), || {
        let m = engine.meters();
        m.dsp.voiced && (m.dsp.detected_hz - 220.0).abs() < 2.0
    }));
    assert_eq!(engine.health(), EngineHealth::Healthy);
}

#[test]
fn fr03_monitor_toggle_mutes_headphones_but_not_cable() {
    let b = MockBackend::with_default_devices();
    b.set_signal(MockSignal::Sine {
        hz: 220.0,
        amp: 0.3,
    });
    let controls = Arc::new(SharedControls::default());
    let engine = Engine::start(&b, &headset_config(), controls.clone()).unwrap();
    assert!(wait_until(Duration::from_secs(2), || b
        .first_sound("hp")
        .is_some()));
    controls.set_monitor(false, 1.0);
    std::thread::sleep(Duration::from_millis(300));
    b.clear_rendered();
    std::thread::sleep(Duration::from_millis(200));
    let hp = b.rendered("hp");
    assert!(!hp.is_empty() && hp.iter().all(|v| v.abs() < 1e-4));
    assert!(b.rendered("cable-in").iter().any(|v| v.abs() > 0.05));
    drop(engine);
}

#[test]
fn fr03_fr10_bypass_mutes_headphones_and_sends_dry_voice_to_cable() {
    let b = MockBackend::with_default_devices();
    b.set_signal(MockSignal::Sine {
        hz: 220.0,
        amp: 0.3,
    });
    let controls = Arc::new(SharedControls::new(
        &TuningParams {
            bypass: true,
            ..Default::default()
        },
        true,
        1.0,
    ));
    let engine = Engine::start(&b, &headset_config(), controls.clone()).unwrap();
    assert!(wait_until(Duration::from_secs(2), || b
        .rendered("cable-in")
        .iter()
        .any(|v| v.abs() > 0.05)));
    let hp = b.rendered("hp");
    assert!(!hp.is_empty() && hp.iter().all(|v| v.abs() < 1e-4));
    // Tuning on (the hotkey): the headphones carry the voice.
    controls.set_bypass(false);
    assert!(wait_until(Duration::from_secs(2), || b
        .rendered("hp")
        .iter()
        .any(|v| v.abs() > 0.05)));
    drop(engine);
}

#[test]
fn fr04_missing_cable_is_reported_but_engine_runs() {
    let b = MockBackend::with_default_devices();
    let cfg = EngineConfig {
        cable_device: Some("no-such-cable".into()),
        ..headset_config()
    };
    let engine = Engine::start(&b, &cfg, Arc::new(SharedControls::default())).unwrap();
    assert!(engine.status().cable.is_none());
    assert!(engine.status().cable_error.is_some());
}

#[test]
fn fr11_live_params_reach_the_audio_thread() {
    let b = MockBackend::with_default_devices();
    b.set_signal(MockSignal::Sine {
        hz: 226.0,
        amp: 0.3,
    });
    let controls = Arc::new(SharedControls::new(
        &TuningParams {
            retune_ms: 0.0,
            ..Default::default()
        },
        true,
        1.0,
    ));
    let mut engine = Engine::start(&b, &headset_config(), controls.clone()).unwrap();
    assert!(wait_until(Duration::from_secs(2), || engine
        .meters()
        .dsp
        .correction_cents
        .abs()
        > 20.0));
    controls.set_params(&TuningParams {
        mix: 0.0,
        bypass: true,
        ..Default::default()
    });
    // Bypass keeps analysing, so meters stay live, and params round-trip.
    assert!(controls.params().bypass);
    assert!(wait_until(Duration::from_secs(1), || engine
        .meters()
        .dsp
        .voiced));
}

#[test]
fn fr16_latency_test_measures_loopback_delay() {
    let mut b = MockBackend::with_default_devices();
    // Short periods keep ring/queue quantisation small.
    b.period_frames = 96;
    b.set_signal(MockSignal::Loopback { delay_frames: 0 });
    let cfg = EngineConfig {
        cable_device: None,
        ..headset_config()
    };
    let mut engine = Engine::start(&b, &cfg, Arc::new(SharedControls::default())).unwrap();
    std::thread::sleep(Duration::from_millis(150));
    let base = engine.measure_latency(Duration::from_secs(5)).unwrap();
    // `Ok` already means the correlation peak passed the analyzer threshold.
    assert!(base.total_ms > base.hardware_ms);
    // +50 ms of "air" on the same running streams (same thread phase).
    b.set_air_delay(2_400);
    std::thread::sleep(Duration::from_millis(100));
    let delayed = engine.measure_latency(Duration::from_secs(5)).unwrap();
    // The exact lag math is unit-tested in `latency::tests`. Here the mock
    // threads are not real-time, so under CPU load they can underrun and
    // re-prime (shifting by whole periods); assert the plumbing with bounds
    // that hold under jitter: the added air shows up, nothing is lost.
    let ring_and_buffers_ms = 40.0;
    assert!(base.hardware_ms < ring_and_buffers_ms, "{base:?}");
    assert!(
        delayed.hardware_ms >= 50.0 && delayed.hardware_ms < 50.0 + ring_and_buffers_ms,
        "base {base:?}, delayed {delayed:?}"
    );
}

#[test]
fn fr16_latency_test_without_loopback_fails_cleanly() {
    let b = MockBackend::with_default_devices();
    b.set_signal(MockSignal::Silence);
    let mut engine =
        Engine::start(&b, &headset_config(), Arc::new(SharedControls::default())).unwrap();
    let r = engine.measure_latency(Duration::from_secs(5));
    assert!(
        matches!(r, Err(tuner_engine::EngineError::NoLoopbackSignal)),
        "{r:?}"
    );
}

#[test]
fn fr05_device_loss_is_reported() {
    let b = MockBackend::with_default_devices();
    let engine = Engine::start(&b, &headset_config(), Arc::new(SharedControls::default())).unwrap();
    b.unplug("mic");
    assert!(wait_until(Duration::from_secs(1), || engine.health()
        == EngineHealth::CaptureLost));
}

fn two_mic_backend() -> MockBackend {
    let b = MockBackend::with_default_devices();
    let mut devices = b.list_devices().unwrap();
    devices.push(DeviceInfo {
        id: "mic2".into(),
        name: "USB Microphone (Mock)".into(),
        direction: Direction::Capture,
        is_default: false,
        is_default_communications: false,
        is_vb_cable: false,
        is_bluetooth: false,
        asio_driver: None,
        container_id: None,
    });
    let b2 = MockBackend::new(devices);
    b2.set_signal(MockSignal::Sine {
        hz: 200.0,
        amp: 0.2,
    });
    b2
}

#[test]
fn fr05_supervisor_recovers_from_unplug_and_replug_within_2s() {
    let b = Arc::new(two_mic_backend());
    let mut sup = Supervisor::new(b.clone(), Arc::new(SharedControls::default()));
    sup.start(headset_config()).unwrap();
    assert_eq!(sup.status().state, SupervisorState::Running);

    // Unplug the chosen mic: must be running on a substitute within 2 s.
    b.unplug("mic");
    let t0 = Instant::now();
    assert!(wait_until(Duration::from_secs(3), || {
        sup.tick();
        sup.engine()
            .is_some_and(|e| e.status().capture.device_id == "mic2")
    }));
    assert!(t0.elapsed() < Duration::from_secs(2), "{:?}", t0.elapsed());
    assert!(sup.status().using_fallback_device);
    let cap = &sup.engine().unwrap().status().capture;
    assert_ne!(
        cap.device_id, "cable-out",
        "never fall back to the virtual cable"
    );

    // Replug: back on the chosen mic within 2 s.
    b.replug("mic");
    sup.on_device_change();
    let t1 = Instant::now();
    assert!(wait_until(Duration::from_secs(3), || {
        sup.tick();
        sup.engine()
            .is_some_and(|e| e.status().capture.device_id == "mic")
    }));
    assert!(t1.elapsed() < Duration::from_secs(2), "{:?}", t1.elapsed());
    assert!(!sup.status().using_fallback_device);
    assert!(sup.status().restarts >= 2);
    sup.stop();
    assert_eq!(sup.status().state, SupervisorState::Stopped);
}

#[test]
fn fr05_supervisor_waits_when_no_microphone_then_recovers() {
    let b = Arc::new(MockBackend::with_default_devices());
    let mut sup = Supervisor::new(b.clone(), Arc::new(SharedControls::default()));
    b.unplug("mic");
    assert!(sup.start(headset_config()).is_err());
    assert_eq!(sup.status().state, SupervisorState::Recovering);
    b.replug("mic");
    sup.on_device_change();
    assert!(wait_until(Duration::from_secs(2), || {
        sup.tick();
        sup.status().state == SupervisorState::Running
    }));
}
