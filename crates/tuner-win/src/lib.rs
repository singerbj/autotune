//! `tuner-win`: everything that touches Windows audio policy.
//!
//! * [`routing`] — "Use for all apps" (FR-14): set CABLE Output as the default
//!   recording device with a crash-safe backup/restore (NFR-08). The
//!   undocumented `IPolicyConfig` lives only here, behind the
//!   [`DefaultEndpointControl`] trait, and every failure has a fallback (the
//!   wizard's manual instructions).
//! * [`setup`] — pure evaluation of the setup check (FR-12, FR-13): VB-Cable
//!   presence, cable conflicts, Discord session, Bluetooth/sidetone warnings.
//! * Windows-only: device change notifications, audio session enumeration,
//!   and the real `IPolicyConfig` implementation.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod routing;
pub mod setup;

#[cfg(windows)]
mod win;

pub use routing::{DefaultEndpointControl, Role, RouteAllApps, RoutingBackup, RoutingStore};
pub use setup::{
    evaluate_setup, is_discord_process, AudioSession, EndpointSummary, SessionState, SetupInputs,
    SetupReport,
};

/// Errors from Windows integration.
#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum WinError {
    #[error("not supported on this system: {0}")]
    Unsupported(&'static str),
    #[error("{context} failed (0x{code:08X})")]
    Os { context: &'static str, code: u32 },
    #[error("could not save the routing backup: {0}")]
    Store(String),
    #[error("device not found: {0}")]
    NotFound(String),
}

/// Device change notification (FR-05 input to the supervisor).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeviceEvent {
    Added(String),
    Removed(String),
    StateChanged(String),
    DefaultChanged { capture: bool, id: String },
}

/// The platform's default-endpoint controller. On Windows this is the
/// `IPolicyConfig` implementation; elsewhere it reports `Unsupported`.
pub fn default_endpoint_control() -> Box<dyn DefaultEndpointControl> {
    #[cfg(windows)]
    {
        Box::new(win::policy::PolicyConfigControl)
    }
    #[cfg(not(windows))]
    {
        Box::new(routing::UnsupportedControl)
    }
}

/// Watches audio endpoints; drop to unregister.
pub struct DeviceWatcher {
    #[cfg(windows)]
    _inner: win::notify::Watcher,
}

impl DeviceWatcher {
    /// Start watching. `on_event` runs on a COM notification thread and must
    /// be quick (e.g. push into a channel).
    pub fn start(on_event: impl Fn(DeviceEvent) + Send + Sync + 'static) -> Result<Self, WinError> {
        #[cfg(windows)]
        {
            Ok(Self {
                _inner: win::notify::Watcher::start(Box::new(on_event))?,
            })
        }
        #[cfg(not(windows))]
        {
            let _ = on_event;
            Ok(Self {})
        }
    }
}

/// Audio sessions currently open on an endpoint (Discord detection, cable
/// conflicts). Empty on non-Windows.
pub fn endpoint_sessions(endpoint_id: &str) -> Result<Vec<AudioSession>, WinError> {
    #[cfg(windows)]
    {
        win::sessions::list(endpoint_id)
    }
    #[cfg(not(windows))]
    {
        let _ = endpoint_id;
        Ok(Vec::new())
    }
}

/// Whether "Listen to this device" is enabled on a capture endpoint (it
/// doubles the user's voice in their headphones). `None` when unknown.
pub fn listen_to_device_enabled(endpoint_id: &str) -> Option<bool> {
    #[cfg(windows)]
    {
        win::sessions::listen_enabled(endpoint_id)
    }
    #[cfg(not(windows))]
    {
        let _ = endpoint_id;
        None
    }
}
