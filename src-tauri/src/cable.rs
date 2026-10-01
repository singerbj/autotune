//! VB-Cable lifecycle in the app (ADR 0012): keep Windows' default devices
//! off the cable around installs and removals, follow the cable appearing
//! and disappearing, and repair it from the setup check.
//!
//! The installer runs the `--cable` helpers below (see
//! `installer/resources/install-vbcable.ps1` and `installer/nsis/hooks.nsh`):
//!
//! * `snapshot`: before VB-Cable is installed, save the default devices to
//!   the signed-in user's [`pending_path`].
//! * `settle`: once the cable is present, put back every default the install
//!   moved onto it. The app does the same at launch and on device changes,
//!   as the user, which covers installs that only finish after a reboot.
//! * `users <file>`: write the apps recording from CABLE Output, for the
//!   uninstaller's "remove VB-Cable?" question.
//! * `release`: before VB-Cable is removed, move every default off it.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use specta::Type;
use tuner_audio::{AudioBackend, DeviceInfo, Direction};
use tuner_engine::SupervisorState;
use tuner_win::cable::{apply, defaults_to_restore, release_plan};
use tuner_win::{DefaultEndpointControl, DefaultsSnapshot, EndpointSummary, Flow};

use crate::error::{AppError, AppResult, ErrorKind};
use crate::state::AppState;

/// After the cable first shows up, keep undoing Windows' default changes for
/// this long: Windows may switch the default a moment after the endpoints
/// arrive. Past it, a cable default is the user's own choice.
const SETTLE_GRACE_SECS: u64 = 120;

/// The defaults from before a VB-Cable install, until they are settled.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingInstall {
    pub before: DefaultsSnapshot,
    /// Unix seconds when the cable was first seen present.
    pub cable_seen_at: Option<u64>,
}

/// What `repair_virtual_mic` achieved.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum RepairOutcome {
    /// Both cable endpoints are active.
    Fixed,
    /// VB-Cable was installed or repaired but Windows needs a restart.
    RestartRequired,
}

/// `%APPDATA%\<identifier>\cable-install-defaults.json` of the signed-in user
/// (also when the installer runs elevated as another account).
pub fn pending_path() -> PathBuf {
    crate::user_dirs()
        .roaming
        .join(crate::IDENTIFIER)
        .join("cable-install-defaults.json")
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

pub fn summaries(devices: &[DeviceInfo]) -> Vec<EndpointSummary> {
    devices
        .iter()
        .map(|d| EndpointSummary {
            id: d.id.clone(),
            name: d.name.clone(),
            is_capture: d.direction == Direction::Capture,
            is_bluetooth: d.is_bluetooth,
            is_vb_cable: d.is_vb_cable,
        })
        .collect()
}

fn cable_present(endpoints: &[EndpointSummary]) -> bool {
    let side = |capture| {
        endpoints
            .iter()
            .any(|e| e.is_vb_cable && e.is_capture == capture)
    };
    side(true) && side(false)
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(value)?)?;
    std::fs::rename(&tmp, path)
}

/// Save the current defaults before VB-Cable is installed.
pub fn snapshot_pending(path: &Path, control: &dyn DefaultEndpointControl) -> AppResult<()> {
    let pending = PendingInstall {
        before: DefaultsSnapshot::read(control)?,
        cable_seen_at: None,
    };
    write_json(path, &pending)?;
    Ok(())
}

/// Undo the default changes a VB-Cable install made, if one is pending.
/// Returns how many defaults were put back.
pub fn settle_pending(
    path: &Path,
    control: &dyn DefaultEndpointControl,
    endpoints: &[EndpointSummary],
    now: u64,
) -> AppResult<usize> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Ok(0);
    };
    let Ok(mut pending) = serde_json::from_str::<PendingInstall>(&text) else {
        let _ = std::fs::remove_file(path);
        return Ok(0);
    };
    if !cable_present(endpoints) {
        return Ok(0);
    }
    if pending
        .cable_seen_at
        .is_some_and(|t| now.saturating_sub(t) > SETTLE_GRACE_SECS)
    {
        std::fs::remove_file(path)?;
        return Ok(0);
    }
    let current = DefaultsSnapshot::read(control)?;
    let fix = defaults_to_restore(&pending.before, &current, endpoints);
    apply(control, &fix)?;
    if pending.cable_seen_at.is_none() {
        pending.cable_seen_at = Some(now);
        write_json(path, &pending)?;
    }
    Ok(fix.len())
}

/// Move the given flows' defaults off VB-Cable. Returns how many moved.
pub fn release(
    control: &dyn DefaultEndpointControl,
    endpoints: &[EndpointSummary],
    flows: &[Flow],
) -> AppResult<usize> {
    let current = DefaultsSnapshot::read(control)?;
    let plan = release_plan(&current, endpoints, flows);
    apply(control, &plan)?;
    Ok(plan.len())
}

/// Apps recording from CABLE Output, other than TunedUp.
pub fn cable_users(devices: &[DeviceInfo]) -> Vec<String> {
    devices
        .iter()
        .find(|d| d.is_cable_output())
        .and_then(|d| tuner_win::endpoint_sessions(&d.id).ok())
        .map(|s| tuner_win::session_apps(&s, std::process::id()))
        .unwrap_or_default()
}

/// The installer's helper script, next to the installed app.
pub fn helper_script() -> Option<PathBuf> {
    let script = std::env::current_exe()
        .ok()?
        .parent()?
        .join("installer")
        .join("install-vbcable.ps1");
    (cfg!(windows) && script.is_file()).then_some(script)
}

/// Undo default changes from a recent VB-Cable install (at launch and on
/// device changes).
pub fn settle_install(s: &AppState, devices: &[DeviceInfo]) {
    match settle_pending(
        &pending_path(),
        s.endpoint_control.as_ref(),
        &summaries(devices),
        now_secs(),
    ) {
        Ok(0) => {}
        Ok(n) => tracing::info!("put back {n} default device(s) VB-Cable took over"),
        Err(e) => tracing::warn!("settling defaults after the VB-Cable install failed: {e}"),
    }
}

/// Device list changed: settle a pending install, and keep "Use for all
/// apps" in step with the cable (FR-14): give the user's mic back when the
/// cable disappears, and re-apply it when the cable returns while the tuner
/// runs (it is otherwise applied when the tuner starts).
pub fn on_devices_changed(s: &AppState, devices: &[DeviceInfo]) {
    settle_install(s, devices);
    if !s.config().route_all_apps {
        return;
    }
    let routing = s.routing();
    let active = routing.is_active().unwrap_or(false);
    match (devices.iter().find(|d| d.is_cable_output()), active) {
        (None, true) => {
            if let Err(e) = routing.disable() {
                tracing::warn!("restoring the default mic after VB-Cable went away failed: {e}");
            }
        }
        (Some(cable), false) => {
            let running =
                crate::state::lock(&s.supervisor).status().state != SupervisorState::Stopped;
            if running {
                if let Err(e) = routing.enable(&cable.id) {
                    tracing::warn!("re-applying 'use for all apps' failed: {e}");
                }
            }
        }
        _ => {}
    }
}

/// Make the cable usable: turn disabled endpoints back on, then (elevated)
/// install or restart VB-Cable if it is still missing.
pub fn repair(s: &AppState) -> AppResult<RepairOutcome> {
    let control = s.endpoint_control.as_ref();
    let present = |b: &dyn AudioBackend| -> AppResult<bool> {
        Ok(cable_present(&summaries(&b.list_devices()?)))
    };
    let had_disabled = s
        .backend
        .inactive_vb_cable_endpoints()
        .unwrap_or_default()
        .iter()
        .any(|e| e.disabled);
    for e in s.backend.inactive_vb_cable_endpoints().unwrap_or_default() {
        if e.disabled {
            if let Err(err) = control.set_endpoint_enabled(&e.id, true) {
                tracing::warn!("enabling {} failed: {err}", e.name);
            }
        }
    }
    if had_disabled {
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    if !present(s.backend.as_ref())? {
        let Some(script) = helper_script() else {
            return Err(AppError::new(
                ErrorKind::Unsupported,
                "TunedUp can't install VB-Cable from here. Re-run the TunedUp installer, or \
                 install VB-Cable from vb-audio.com and restart Windows.",
            ));
        };
        // The user's own defaults, not the elevated account's.
        if let Err(e) = snapshot_pending(&pending_path(), control) {
            tracing::warn!("saving default devices before the repair failed: {e}");
        }
        let code = run_helper(&script)?;
        tracing::info!("VB-Cable repair helper exited with {code}");
        match code {
            0 => {}
            3010 => return Ok(RepairOutcome::RestartRequired),
            _ => {
                return Err(AppError::new(
                    ErrorKind::Device,
                    "VB-Cable couldn't be installed or restarted. Details are in \
                     %TEMP%\\tunedup-vbcable.log.",
                ))
            }
        }
    }
    // Endpoints can take a moment to show up after the helper or an enable.
    for _ in 0..10 {
        let devices = s.devices()?;
        if cable_present(&summaries(&devices)) {
            on_devices_changed(s, &devices);
            return Ok(RepairOutcome::Fixed);
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    Ok(RepairOutcome::RestartRequired)
}

fn run_helper(script: &Path) -> AppResult<u32> {
    let windir = std::env::var_os("WINDIR").unwrap_or_else(|| r"C:\Windows".into());
    let powershell = PathBuf::from(windir).join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
    let dir = script.parent().unwrap_or(script);
    let args = format!(
        r#"-NoProfile -NonInteractive -ExecutionPolicy Bypass -File "{}" -Action Repair -PackDir "{}" -RegistryKey "HKLM:\Software\TunedUp""#,
        script.display(),
        dir.join("vbcable").display(),
    );
    Ok(tuner_win::run_elevated(&powershell, &args)?)
}

/// `--cable <verb> [file]` for the installer. Returns the exit code.
pub fn run_cli(args: &[String]) -> i32 {
    let mut rest = args.iter().skip_while(|a| *a != "--cable").skip(1);
    let verb = rest.next().map(String::as_str).unwrap_or_default();
    let file = rest.next();
    let control = tuner_win::default_endpoint_control();
    let backend = tuner_audio::default_backend();
    let devices = || backend.list_devices().map_err(AppError::from);
    let result: AppResult<()> = match verb {
        "snapshot" => snapshot_pending(&pending_path(), control.as_ref()),
        "settle" => devices().and_then(|d| {
            let n = settle_pending(
                &pending_path(),
                control.as_ref(),
                &summaries(&d),
                now_secs(),
            )?;
            tracing::info!("cable settle: put back {n} default device(s)");
            Ok(())
        }),
        "users" => match file {
            Some(f) => devices().and_then(|d| {
                std::fs::write(f, cable_users(&d).join(", ")).map_err(AppError::from)
            }),
            None => Err(AppError::new(
                ErrorKind::Config,
                "usage: --cable users <file>",
            )),
        },
        "release" => devices().and_then(|d| {
            let n = release(
                control.as_ref(),
                &summaries(&d),
                &[Flow::Render, Flow::Capture],
            )?;
            tracing::info!("cable release: moved {n} default device(s) off VB-Cable");
            Ok(())
        }),
        other => Err(AppError::new(
            ErrorKind::Config,
            format!("unknown --cable action \"{other}\""),
        )),
    };
    match result {
        Ok(()) => 0,
        Err(e) => {
            tracing::error!("--cable {verb} failed: {e}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;
    use tuner_win::{Role, WinError, ALL_ROLES};

    struct FakeOs {
        endpoints: Vec<EndpointSummary>,
        defaults: Mutex<HashMap<(Flow, Role), String>>,
    }

    impl FakeOs {
        fn new() -> Self {
            let ep = |id: &str, capture, cable| EndpointSummary {
                id: id.into(),
                name: id.into(),
                is_capture: capture,
                is_bluetooth: false,
                is_vb_cable: cable,
            };
            let os = Self {
                endpoints: vec![
                    ep("mic", true, false),
                    ep("hp", false, false),
                    ep("cable-in", false, true),
                    ep("cable-out", true, true),
                ],
                defaults: Mutex::new(HashMap::new()),
            };
            os.set_all(Flow::Render, "hp");
            os.set_all(Flow::Capture, "mic");
            os
        }
        fn set_all(&self, flow: Flow, id: &str) {
            for r in ALL_ROLES {
                self.defaults.lock().unwrap().insert((flow, r), id.into());
            }
        }
        fn get(&self, flow: Flow, role: Role) -> Option<String> {
            self.defaults.lock().unwrap().get(&(flow, role)).cloned()
        }
        fn without_cable(&self) -> Vec<EndpointSummary> {
            self.endpoints
                .iter()
                .filter(|e| !e.is_vb_cable)
                .cloned()
                .collect()
        }
    }

    impl DefaultEndpointControl for FakeOs {
        fn get_default_capture(&self, role: Role) -> Result<Option<String>, WinError> {
            Ok(self.get(Flow::Capture, role))
        }
        fn set_default_capture(&self, id: &str, role: Role) -> Result<(), WinError> {
            let e = self.endpoints.iter().find(|e| e.id == id).unwrap();
            let flow = if e.is_capture {
                Flow::Capture
            } else {
                Flow::Render
            };
            self.defaults
                .lock()
                .unwrap()
                .insert((flow, role), id.into());
            Ok(())
        }
        fn get_default(&self, flow: Flow, role: Role) -> Result<Option<String>, WinError> {
            Ok(self.get(flow, role))
        }
    }

    #[test]
    fn fr15_pending_install_is_settled_once_the_cable_appears() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pending.json");
        let os = FakeOs::new();
        snapshot_pending(&path, &os).unwrap();

        // Before the reboot: no cable yet, nothing to do, keep waiting.
        assert_eq!(
            settle_pending(&path, &os, &os.without_cable(), 1000).unwrap(),
            0
        );
        assert!(path.exists());

        // After it: Windows made the cable the speakers.
        os.set_all(Flow::Render, "cable-in");
        assert_eq!(settle_pending(&path, &os, &os.endpoints, 2000).unwrap(), 3);
        assert_eq!(os.get(Flow::Render, Role::Console).as_deref(), Some("hp"));

        // Windows switches again a moment later: still within the grace period.
        os.set_all(Flow::Render, "cable-in");
        assert_eq!(settle_pending(&path, &os, &os.endpoints, 2030).unwrap(), 3);

        // Much later the user picks the cable on purpose: left alone, file gone.
        os.set_all(Flow::Render, "cable-in");
        let later = 2000 + SETTLE_GRACE_SECS + 1;
        assert_eq!(settle_pending(&path, &os, &os.endpoints, later).unwrap(), 0);
        assert_eq!(
            os.get(Flow::Render, Role::Console).as_deref(),
            Some("cable-in")
        );
        assert!(!path.exists());
    }

    #[test]
    fn settle_without_a_pending_install_does_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let os = FakeOs::new();
        os.set_all(Flow::Render, "cable-in");
        let path = dir.path().join("none.json");
        assert_eq!(settle_pending(&path, &os, &os.endpoints, 1).unwrap(), 0);
        assert_eq!(
            os.get(Flow::Render, Role::Console).as_deref(),
            Some("cable-in")
        );
    }

    #[test]
    fn corrupt_pending_file_is_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pending.json");
        std::fs::write(&path, "{ nope").unwrap();
        let os = FakeOs::new();
        assert_eq!(settle_pending(&path, &os, &os.endpoints, 1).unwrap(), 0);
        assert!(!path.exists());
    }

    #[test]
    fn release_before_uninstall_moves_every_default_off_the_cable() {
        let os = FakeOs::new();
        os.set_all(Flow::Render, "cable-in");
        os.set_all(Flow::Capture, "cable-out");
        assert_eq!(
            release(&os, &os.endpoints, &[Flow::Render, Flow::Capture]).unwrap(),
            6
        );
        for r in ALL_ROLES {
            assert_eq!(os.get(Flow::Render, r).as_deref(), Some("hp"));
            assert_eq!(os.get(Flow::Capture, r).as_deref(), Some("mic"));
        }
    }

    #[test]
    fn unknown_cli_action_fails() {
        let args = ["tunedup".to_string(), "--cable".into(), "bogus".into()];
        assert_eq!(run_cli(&args), 1);
    }
}
