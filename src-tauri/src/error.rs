//! Typed errors mapped to user-facing messages for the UI.

use serde::Serialize;
use specta::Type;
use tuner_audio::AudioError;
use tuner_engine::EngineError;
use tuner_win::WinError;

/// Error returned by every command. `message` is shown to the user as-is.
#[derive(Debug, Clone, Serialize, Type, thiserror::Error)]
#[serde(rename_all = "camelCase")]
#[error("{message}")]
pub struct AppError {
    pub kind: ErrorKind,
    pub message: String,
}

#[derive(Debug, Clone, Copy, Serialize, Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ErrorKind {
    Device,
    DeviceInUse,
    NotRunning,
    Latency,
    Unsupported,
    Config,
    Update,
    Internal,
}

impl AppError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl From<AudioError> for AppError {
    fn from(e: AudioError) -> Self {
        let kind = match e {
            AudioError::DeviceInUse => ErrorKind::DeviceInUse,
            AudioError::TierUnavailable(_) => ErrorKind::Unsupported,
            _ => ErrorKind::Device,
        };
        let message = match &e {
            AudioError::DeviceInUse => {
                "Another app is using the microphone exclusively. Close it or pick another mic."
                    .into()
            }
            AudioError::NoDevice => {
                "No audio device found. Plug in a headset and try again.".into()
            }
            AudioError::DeviceNotFound(_) => "The selected device is not connected.".into(),
            other => other.to_string(),
        };
        Self { kind, message }
    }
}

impl From<EngineError> for AppError {
    fn from(e: EngineError) -> Self {
        match e {
            EngineError::Audio(a) => a.into(),
            EngineError::NotRunning => Self::new(ErrorKind::NotRunning, "Start the tuner first."),
            EngineError::NoLoopbackSignal | EngineError::LatencyTimeout | EngineError::Busy => {
                Self::new(ErrorKind::Latency, e.to_string())
            }
            EngineError::NoCaptureDevice => Self::new(ErrorKind::Device, e.to_string()),
            EngineError::Dsp(d) => Self::new(ErrorKind::Internal, d.to_string()),
        }
    }
}

impl From<WinError> for AppError {
    fn from(e: WinError) -> Self {
        match e {
            WinError::Unsupported(_) => Self::new(
                ErrorKind::Unsupported,
                "Windows did not allow changing the default microphone. Set \"CABLE Output\" as the \
                 default recording device in Sound settings instead.",
            ),
            other => Self::new(ErrorKind::Internal, other.to_string()),
        }
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        Self::new(ErrorKind::Config, e.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        Self::new(ErrorKind::Config, e.to_string())
    }
}

impl From<tauri::Error> for AppError {
    fn from(e: tauri::Error) -> Self {
        Self::new(ErrorKind::Internal, e.to_string())
    }
}

impl From<tauri_plugin_updater::Error> for AppError {
    fn from(e: tauri_plugin_updater::Error) -> Self {
        Self::new(ErrorKind::Update, e.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_audio_errors_to_friendly_messages() {
        let e: AppError = EngineError::Audio(AudioError::DeviceInUse).into();
        assert_eq!(e.kind, ErrorKind::DeviceInUse);
        assert!(e.message.contains("exclusively"));
        let e: AppError = WinError::Unsupported("x").into();
        assert_eq!(e.kind, ErrorKind::Unsupported);
        assert!(e.message.contains("CABLE Output"));
    }

    #[test]
    fn serializes_camel_case() {
        let e = AppError::new(ErrorKind::NotRunning, "m");
        assert_eq!(
            serde_json::to_string(&e).unwrap(),
            r#"{"kind":"notRunning","message":"m"}"#
        );
    }
}
