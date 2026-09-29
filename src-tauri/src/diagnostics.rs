//! Diagnostics panel data and the "Copy diagnostics" report (FR-18).

use std::fmt::Write;

use serde::Serialize;
use specta::Type;
use tuner_engine::{EngineStatus, MeterSnapshot, SupervisorStatus};

#[derive(Serialize, Debug, Clone, Type)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostics {
    pub app_version: String,
    pub os: String,
    pub audio_backend: String,
    pub supervisor: SupervisorStatus,
    pub engine: Option<EngineStatus>,
    pub meters: Option<MeterSnapshot>,
    pub measured_latency_ms: Option<f32>,
    pub log_dir: String,
    pub config_path: String,
    /// Plain-text report for support.
    pub report: String,
}

pub fn report(d: &Diagnostics) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "TunedUp {} on {}", d.app_version, d.os);
    let _ = writeln!(s, "Backend: {}", d.audio_backend);
    let _ = writeln!(
        s,
        "Supervisor: {:?}, restarts {}, fallback device {}",
        d.supervisor.state, d.supervisor.restarts, d.supervisor.using_fallback_device
    );
    if let Some(err) = &d.supervisor.last_error {
        let _ = writeln!(s, "Last error: {err}");
    }
    if let Some(e) = &d.engine {
        for (label, st) in [
            ("Capture", Some(&e.capture)),
            ("Monitor", Some(&e.monitor)),
            ("Cable", e.cable.as_ref()),
        ] {
            if let Some(st) = st {
                let _ = writeln!(
                    s,
                    "{label}: {} [{}] {} Hz, {} ch, period {} frames ({:.2} ms), stream latency {:.2} ms",
                    st.device_name,
                    st.tier,
                    st.sample_rate,
                    st.channels,
                    st.period_frames,
                    st.period_ms(),
                    st.stream_latency_ms()
                );
                for f in &st.fallbacks {
                    let _ = writeln!(s, "  fell back from {}: {}", f.tier, f.error);
                }
            }
        }
        if let Some(err) = &e.cable_error {
            let _ = writeln!(s, "Cable: unavailable ({err})");
        }
        let _ = writeln!(
            s,
            "Latency: DSP {:.1} ms, estimated round trip {:.1} ms, monitor resampling {}",
            e.dsp_latency_ms, e.estimated_latency_ms, e.monitor_resampling
        );
    }
    if let Some(ms) = d.measured_latency_ms {
        let _ = writeln!(s, "Measured round trip: {ms:.1} ms");
    }
    if let Some(m) = &d.meters {
        let _ = writeln!(
            s,
            "Xruns: {} (capture gaps {}, monitor underruns {}, cable underruns {}, ring overflows {})",
            m.xruns, m.capture_discontinuities, m.monitor_underruns, m.cable_underruns, m.ring_overflows
        );
        let _ = writeln!(
            s,
            "Callback: p99 {} µs, max {} µs, budget {} µs; monitor fill {:.1} ms, cable fill {:.1} ms, cable ratio {:.5}",
            m.callback_p99_us,
            m.callback_max_us,
            m.callback_budget_us,
            m.monitor_fill_ms,
            m.cable_fill_ms,
            m.cable_ratio
        );
    }
    let _ = writeln!(s, "Logs: {}", d.log_dir);
    let _ = writeln!(s, "Config: {}", d.config_path);
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use tuner_engine::SupervisorState;

    #[test]
    fn fr18_report_contains_key_facts() {
        let d = Diagnostics {
            app_version: "1.2.3".into(),
            os: "windows x86_64".into(),
            audio_backend: "wasapi".into(),
            supervisor: SupervisorStatus {
                state: SupervisorState::Running,
                last_error: None,
                restarts: 1,
                using_fallback_device: false,
            },
            engine: None,
            meters: Some(MeterSnapshot {
                xruns: 3,
                callback_p99_us: 40,
                callback_max_us: 120,
                ..Default::default()
            }),
            measured_latency_ms: Some(17.5),
            log_dir: "C:\\logs".into(),
            config_path: "C:\\cfg".into(),
            report: String::new(),
        };
        let r = report(&d);
        assert!(r.contains("TunedUp 1.2.3"));
        assert!(r.contains("Xruns: 3"));
        assert!(r.contains("p99 40 µs, max 120 µs"));
        assert!(r.contains("Measured round trip: 17.5 ms"));
    }
}
