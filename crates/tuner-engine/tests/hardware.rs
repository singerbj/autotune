//! Manual hardware checks on Windows (ignored by default).
//!
//! ```text
//! # M2: passthrough-equivalent soak with default devices (bypass on)
//! set TUNER_SOAK_SECS=3600 && cargo test -p tuner-engine --release --test hardware -- --ignored --nocapture soak
//! # FR-16: loopback latency (hold an earcup to the mic)
//! cargo test -p tuner-engine --release --test hardware -- --ignored --nocapture latency
//! ```

#![cfg(windows)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use tuner_dsp::TuningParams;
use tuner_engine::{Engine, EngineConfig, EngineHealth, SharedControls};

fn start(bypass: bool) -> Engine {
    let backend = tuner_audio::default_backend();
    let devices = backend.list_devices().unwrap();
    let cable = devices
        .iter()
        .find(|d| d.is_cable_input())
        .map(|d| d.id.clone());
    let controls = Arc::new(SharedControls::new(
        &TuningParams {
            bypass,
            ..Default::default()
        },
        true,
        0.8,
    ));
    let e = Engine::start(
        backend.as_ref(),
        &EngineConfig {
            cable_device: cable,
            ..Default::default()
        },
        controls,
    )
    .unwrap();
    println!("{:#?}", e.status());
    e
}

#[test]
#[ignore = "needs audio hardware"]
fn soak_default_devices_reports_xruns() {
    let secs: u64 = std::env::var("TUNER_SOAK_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(30);
    let mut e = start(false);
    let t0 = Instant::now();
    while t0.elapsed() < Duration::from_secs(secs) {
        std::thread::sleep(Duration::from_secs(5));
        let m = e.meters();
        println!(
            "{:>5}s xruns {} p99 {} µs max {} µs (budget {} µs) mon {:.1} ms cable {:.1} ms",
            t0.elapsed().as_secs(),
            m.xruns,
            m.callback_p99_us,
            m.callback_max_us,
            m.callback_budget_us,
            m.monitor_fill_ms,
            m.cable_fill_ms
        );
        assert_eq!(e.health(), EngineHealth::Healthy);
    }
    assert_eq!(e.meters().xruns, 0, "NFR-02: zero xruns");
}

#[test]
#[ignore = "needs audio hardware and an earcup on the mic"]
fn latency_loopback() {
    let mut e = start(true);
    std::thread::sleep(Duration::from_millis(500));
    let r = e.measure_latency(Duration::from_secs(5)).unwrap();
    println!("{r:#?}");
}
