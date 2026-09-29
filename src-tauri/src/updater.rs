//! Auto-update from GitHub Releases (FR-22) via `tauri-plugin-updater`.
//!
//! The release workflow publishes a signed `latest.json`; the app checks it
//! shortly after launch and every few hours, downloads new versions in the
//! background (signature verified by the plugin), and installs on the user's
//! command or when they quit. Installing restores audio defaults first.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;
use tauri_plugin_updater::{Update, UpdaterExt};
use tauri_specta::Event;

use crate::error::{AppError, AppResult, ErrorKind};
use crate::events::UpdateEvent;
use crate::state::{lock, state};

const FIRST_CHECK_DELAY: Duration = Duration::from_secs(20);
const CHECK_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

#[derive(Serialize, Deserialize, Debug, Clone, Type, PartialEq, Default)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum UpdateStatus {
    #[default]
    Idle,
    Checking,
    UpToDate {
        current: String,
    },
    Downloading {
        version: String,
        percent: Option<f32>,
    },
    Ready {
        version: String,
        notes: Option<String>,
    },
    Error {
        message: String,
    },
}

#[derive(Default)]
pub struct UpdaterState {
    pub status: UpdateStatus,
    pub pending: Option<(Update, Vec<u8>)>,
}

fn publish(app: &AppHandle, s: UpdateStatus) {
    lock(&state(app).updater).status = s.clone();
    let _ = UpdateEvent(s).emit(app);
}

/// Check for an update and download it if there is one.
pub async fn check_and_download(app: &AppHandle) -> AppResult<UpdateStatus> {
    if matches!(lock(&state(app).updater).status, UpdateStatus::Ready { .. }) {
        return Ok(lock(&state(app).updater).status.clone());
    }
    publish(app, UpdateStatus::Checking);
    let result = async {
        let Some(update) = app.updater()?.check().await? else {
            return Ok(UpdateStatus::UpToDate {
                current: app.package_info().version.to_string(),
            });
        };
        let version = update.version.clone();
        let notes = update.body.clone();
        let mut received = 0usize;
        let progress_app = app.clone();
        let progress_version = version.clone();
        let bytes = update
            .download(
                move |chunk, total| {
                    received += chunk;
                    let percent = total.map(|t| (received as f64 / t.max(1) as f64 * 100.0) as f32);
                    publish(
                        &progress_app,
                        UpdateStatus::Downloading {
                            version: progress_version.clone(),
                            percent,
                        },
                    );
                },
                || {},
            )
            .await?;
        lock(&state(app).updater).pending = Some((update, bytes));
        Ok::<_, AppError>(UpdateStatus::Ready { version, notes })
    }
    .await;
    let status = match result {
        Ok(s) => s,
        Err(e) => UpdateStatus::Error { message: e.message },
    };
    publish(app, status.clone());
    Ok(status)
}

/// Install the downloaded update. On Windows the plugin launches the signed
/// NSIS installer (passive mode) and exits this process.
pub fn install(app: &AppHandle) -> AppResult<()> {
    let Some((update, bytes)) = lock(&state(app).updater).pending.take() else {
        return Err(AppError::new(
            ErrorKind::Update,
            "No update has been downloaded yet.",
        ));
    };
    crate::shutdown(app);
    update.install(bytes)?;
    app.restart();
}

/// Background loop: first check shortly after launch, then periodically.
pub fn spawn_background_checks(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_CHECK_DELAY).await;
        loop {
            if state(&app).config().auto_update {
                if let Err(e) = check_and_download(&app).await {
                    tracing::warn!("update check failed: {e}");
                }
            }
            tokio::time::sleep(CHECK_INTERVAL).await;
        }
    });
}

/// True when an update is downloaded and waiting (quit installs it).
pub fn has_pending(app: &AppHandle) -> bool {
    lock(&state(app).updater).pending.is_some()
}
