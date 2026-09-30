//! Plain data types shared by all backends.

use core::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum Direction {
    Capture,
    Render,
}

/// Capture fallback chain, best first (FR-02).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum BackendTier {
    Asio,
    WasapiExclusive,
    WasapiSharedLowLatency,
    WasapiShared,
    Mock,
}

impl BackendTier {
    pub const CAPTURE_CHAIN: [BackendTier; 4] = [
        BackendTier::Asio,
        BackendTier::WasapiExclusive,
        BackendTier::WasapiSharedLowLatency,
        BackendTier::WasapiShared,
    ];

    pub fn label(self) -> &'static str {
        match self {
            BackendTier::Asio => "ASIO",
            BackendTier::WasapiExclusive => "WASAPI exclusive",
            BackendTier::WasapiSharedLowLatency => "WASAPI shared (low latency)",
            BackendTier::WasapiShared => "WASAPI shared",
            BackendTier::Mock => "Mock",
        }
    }
}

impl fmt::Display for BackendTier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// One capture or render endpoint (FR-01).
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct DeviceInfo {
    /// Stable endpoint ID (persisted in config).
    pub id: String,
    pub name: String,
    pub direction: Direction,
    pub is_default: bool,
    pub is_default_communications: bool,
    /// "CABLE Input" / "CABLE Output" from VB-Audio.
    pub is_vb_cable: bool,
    pub is_bluetooth: bool,
    /// Name of the ASIO driver that owns this device, if any.
    pub asio_driver: Option<String>,
    /// Physical device container; capture and render endpoints of one USB
    /// headset share it (one clock → no resampling).
    pub container_id: Option<String>,
}

impl DeviceInfo {
    /// True for VB-Cable's render side, which apps send audio into.
    pub fn is_cable_input(&self) -> bool {
        self.is_vb_cable && self.direction == Direction::Render
    }

    /// True for VB-Cable's capture side, which Discord records from.
    pub fn is_cable_output(&self) -> bool {
        self.is_vb_cable && self.direction == Direction::Capture
    }
}

/// Heuristic VB-Cable detection by friendly name.
pub fn is_vb_cable_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    (n.starts_with("cable input") || n.starts_with("cable output")) && n.contains("vb-audio")
        || n.contains("vb-audio virtual cable")
}

/// A VB-Cable endpoint Windows knows about but won't let apps open: turned
/// off in Sound settings, or reported unplugged.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InactiveEndpoint {
    pub name: String,
    pub direction: Direction,
    /// Disabled by the user (otherwise unplugged).
    pub disabled: bool,
}

/// What a running stream actually negotiated.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct StreamInfo {
    pub device_id: String,
    pub device_name: String,
    pub tier: BackendTier,
    pub sample_rate: u32,
    pub channels: u16,
    /// Device period in frames.
    pub period_frames: u32,
    /// Extra latency reported by the stream (`GetStreamLatency`), in frames.
    pub stream_latency_frames: u32,
    /// Tiers tried before this one, with the reason each failed.
    pub fallbacks: Vec<TierAttempt>,
}

impl StreamInfo {
    pub fn period_ms(&self) -> f32 {
        self.period_frames as f32 * 1000.0 / self.sample_rate.max(1) as f32
    }

    pub fn stream_latency_ms(&self) -> f32 {
        self.stream_latency_frames as f32 * 1000.0 / self.sample_rate.max(1) as f32
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TierAttempt {
    pub tier: BackendTier,
    pub error: String,
}

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum AudioError {
    #[error("no audio device available")]
    NoDevice,
    #[error("audio device not found: {0}")]
    DeviceNotFound(String),
    #[error("the device is in use by another application")]
    DeviceInUse,
    #[error("exclusive mode is not allowed for this device")]
    ExclusiveNotAllowed,
    #[error("the device does not support a usable format")]
    FormatNotSupported,
    #[error("the device was unplugged or changed")]
    DeviceInvalidated,
    #[error("tier not available: {0}")]
    TierUnavailable(&'static str),
    #[error("could not open the device with any backend: {}", fmt_attempts(.0))]
    AllTiersFailed(Vec<TierAttempt>),
    #[error("{context}: {message} (0x{code:08X})")]
    Os {
        context: &'static str,
        code: u32,
        message: String,
    },
    #[error("stream thread error: {0}")]
    Thread(String),
}

fn fmt_attempts(a: &[TierAttempt]) -> String {
    a.iter()
        .map(|t| format!("{}: {}", t.tier, t.error))
        .collect::<Vec<_>>()
        .join("; ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fr01_vb_cable_names() {
        assert!(is_vb_cable_name("CABLE Input (VB-Audio Virtual Cable)"));
        assert!(is_vb_cable_name("CABLE Output (VB-Audio Virtual Cable)"));
        assert!(!is_vb_cable_name("Headset Microphone (Arctis 7)"));
        assert!(!is_vb_cable_name("Cable modem speaker"));
        // Adapter name, which survives renaming the endpoint.
        assert!(is_vb_cable_name("VB-Audio Virtual Cable"));
    }

    #[test]
    fn stream_info_ms() {
        let s = StreamInfo {
            device_id: "x".into(),
            device_name: "x".into(),
            tier: BackendTier::WasapiExclusive,
            sample_rate: 48_000,
            channels: 1,
            period_frames: 144,
            stream_latency_frames: 48,
            fallbacks: vec![],
        };
        assert!((s.period_ms() - 3.0).abs() < 1e-6);
        assert!((s.stream_latency_ms() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn all_tiers_failed_message_lists_reasons() {
        let e = AudioError::AllTiersFailed(vec![
            TierAttempt {
                tier: BackendTier::WasapiExclusive,
                error: "in use".into(),
            },
            TierAttempt {
                tier: BackendTier::WasapiShared,
                error: "gone".into(),
            },
        ]);
        let s = e.to_string();
        assert!(s.contains("WASAPI exclusive: in use"));
        assert!(s.contains("WASAPI shared: gone"));
    }
}
