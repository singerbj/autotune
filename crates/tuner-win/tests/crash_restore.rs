//! NFR-08: "No changed default device survives app exit or a crash."
//! Verified by a test that kills the process, then relaunches.
//!
//! The "OS" default-device state is simulated by a JSON file so it survives
//! the killed child process; the routing backup uses the real `FileStore`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use tuner_win::routing::FileStore;
use tuner_win::{DefaultEndpointControl, Role, RouteAllApps, WinError};

struct FileOsDefaults(PathBuf);

impl FileOsDefaults {
    fn read(&self) -> HashMap<String, String> {
        std::fs::read_to_string(&self.0)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }
}

fn key(r: Role) -> &'static str {
    match r {
        Role::Console => "console",
        Role::Multimedia => "multimedia",
        Role::Communications => "communications",
    }
}

impl DefaultEndpointControl for FileOsDefaults {
    fn get_default_capture(&self, role: Role) -> Result<Option<String>, WinError> {
        Ok(self.read().get(key(role)).cloned())
    }
    fn set_default_capture(&self, id: &str, role: Role) -> Result<(), WinError> {
        let mut m = self.read();
        m.insert(key(role).into(), id.into());
        std::fs::write(&self.0, serde_json::to_string(&m).unwrap()).unwrap();
        Ok(())
    }
}

fn paths(dir: &Path) -> (FileOsDefaults, FileStore) {
    (
        FileOsDefaults(dir.join("os-defaults.json")),
        FileStore {
            path: dir.join("config-routing.json"),
        },
    )
}

/// Runs only inside the spawned child: enable routing, then die hard.
#[test]
fn child_enable_then_crash() {
    let Some(dir) = std::env::var_os("NFR08_CHILD_DIR") else {
        return;
    };
    let (os, store) = paths(Path::new(&dir));
    RouteAllApps::new(&os, &store)
        .enable("cable-output")
        .unwrap();
    std::process::abort();
}

#[test]
fn nfr08_killed_process_leaves_no_changed_default_after_relaunch() {
    let dir = tempfile::tempdir().unwrap();
    let (os, store) = paths(dir.path());
    os.set_default_capture("headset-mic", Role::Console)
        .unwrap();
    os.set_default_capture("headset-mic", Role::Communications)
        .unwrap();

    let status = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "child_enable_then_crash",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("NFR08_CHILD_DIR", dir.path())
        .status()
        .unwrap();
    assert!(!status.success(), "child must have been killed");
    assert_eq!(
        os.get_default_capture(Role::Console).unwrap().as_deref(),
        Some("cable-output"),
        "child changed the default before dying"
    );

    // Relaunch: the app's startup path.
    let restored = RouteAllApps::new(&os, &store)
        .recover_after_crash()
        .unwrap();
    assert!(restored);
    assert_eq!(
        os.get_default_capture(Role::Console).unwrap().as_deref(),
        Some("headset-mic")
    );
    assert_eq!(
        os.get_default_capture(Role::Communications)
            .unwrap()
            .as_deref(),
        Some("headset-mic")
    );
}
