//! "Use for all apps" (FR-14) with crash-safe restore (NFR-08).
//!
//! Order of operations is what makes this crash safe:
//! 1. read the current defaults,
//! 2. persist them with `dirty = true` (fsync'd by the store),
//! 3. only then change the defaults.
//!
//! Any launch that finds a dirty backup restores it first, so neither a
//! crash nor a kill can leave the user's microphone hijacked.

use serde::{Deserialize, Serialize};

use crate::WinError;

/// Windows default-device roles (`ERole`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Role {
    Console,
    Multimedia,
    Communications,
}

/// The roles "Use for all apps" changes (console and communications).
pub const ROLES: [Role; 2] = [Role::Console, Role::Communications];

/// Every role, for guarding the defaults around VB-Cable installs.
pub const ALL_ROLES: [Role; 3] = [Role::Console, Role::Multimedia, Role::Communications];

/// Playback (render) or recording (capture) side of the audio system.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Flow {
    Render,
    Capture,
}

/// Reads and writes the default endpoints per role.
pub trait DefaultEndpointControl: Send + Sync {
    fn get_default_capture(&self, role: Role) -> Result<Option<String>, WinError>;
    /// Make `id` the default for `role`; the endpoint's own flow decides
    /// whether that's the default microphone or the default speakers.
    fn set_default_capture(&self, id: &str, role: Role) -> Result<(), WinError>;

    /// The default endpoint of either flow.
    fn get_default(&self, flow: Flow, role: Role) -> Result<Option<String>, WinError> {
        match flow {
            Flow::Capture => self.get_default_capture(role),
            Flow::Render => Err(WinError::Unsupported("reading the default playback device")),
        }
    }

    /// Make `id` (of either flow) the default for `role`.
    fn set_default(&self, id: &str, role: Role) -> Result<(), WinError> {
        self.set_default_capture(id, role)
    }

    /// Turn an endpoint on or off, as Sound settings' Enable/Disable does.
    fn set_endpoint_enabled(&self, id: &str, enabled: bool) -> Result<(), WinError> {
        let _ = (id, enabled);
        Err(WinError::Unsupported("enabling audio devices"))
    }
}

/// Fallback when `IPolicyConfig` is unavailable.
#[derive(Debug, Default, Clone, Copy)]
pub struct UnsupportedControl;

impl DefaultEndpointControl for UnsupportedControl {
    fn get_default_capture(&self, _: Role) -> Result<Option<String>, WinError> {
        Err(WinError::Unsupported(
            "changing the default recording device",
        ))
    }
    fn set_default_capture(&self, _: &str, _: Role) -> Result<(), WinError> {
        Err(WinError::Unsupported(
            "changing the default recording device",
        ))
    }
}

/// Saved defaults, stored in the app config.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct RoutingBackup {
    pub console: Option<String>,
    pub communications: Option<String>,
    /// True while our change is in effect: "restore on next launch".
    pub dirty: bool,
}

/// Durable storage for the backup (the app's config file).
pub trait RoutingStore {
    fn load_backup(&self) -> Result<Option<RoutingBackup>, WinError>;
    /// Persist durably before returning; `None` clears the backup.
    fn save_backup(&self, backup: Option<&RoutingBackup>) -> Result<(), WinError>;
}

/// FR-14 controller.
pub struct RouteAllApps<'a> {
    control: &'a dyn DefaultEndpointControl,
    store: &'a dyn RoutingStore,
}

impl<'a> RouteAllApps<'a> {
    pub fn new(control: &'a dyn DefaultEndpointControl, store: &'a dyn RoutingStore) -> Self {
        Self { control, store }
    }

    /// True while our routing is active.
    pub fn is_active(&self) -> Result<bool, WinError> {
        Ok(self.store.load_backup()?.is_some_and(|b| b.dirty))
    }

    /// Make `cable_output_id` the default recording device for both roles.
    pub fn enable(&self, cable_output_id: &str) -> Result<(), WinError> {
        let backup = match self.store.load_backup()? {
            // Already active: keep the original backup, just re-apply.
            Some(b) if b.dirty => b,
            _ => {
                let get = |r| self.control.get_default_capture(r);
                let b = RoutingBackup {
                    console: get(Role::Console)?.filter(|id| id != cable_output_id),
                    communications: get(Role::Communications)?.filter(|id| id != cable_output_id),
                    dirty: true,
                };
                self.store.save_backup(Some(&b))?;
                b
            }
        };
        for role in ROLES {
            if let Err(e) = self.control.set_default_capture(cable_output_id, role) {
                // Roll back whatever we changed; keep the error for the UI.
                let _ = self.restore(&backup);
                let _ = self.store.save_backup(None);
                return Err(e);
            }
        }
        Ok(())
    }

    /// Restore the saved defaults and clear the backup (on quit or toggle-off).
    pub fn disable(&self) -> Result<(), WinError> {
        if let Some(b) = self.store.load_backup()? {
            self.restore(&b)?;
        }
        self.store.save_backup(None)
    }

    /// Call at every launch (and from the uninstaller): restores defaults left
    /// behind by a crash. Returns true if something was restored.
    pub fn recover_after_crash(&self) -> Result<bool, WinError> {
        match self.store.load_backup()? {
            Some(b) if b.dirty => {
                self.restore(&b)?;
                self.store.save_backup(None)?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    fn restore(&self, b: &RoutingBackup) -> Result<(), WinError> {
        let mut first_err = None;
        for (role, id) in [
            (Role::Console, &b.console),
            (Role::Communications, &b.communications),
        ] {
            if let Some(id) = id {
                if let Err(e) = self.control.set_default_capture(id, role) {
                    first_err.get_or_insert(e);
                }
            }
        }
        first_err.map_or(Ok(()), Err)
    }
}

/// JSON-file store, used by tests and the uninstall helper.
#[derive(Debug, Clone)]
pub struct FileStore {
    pub path: std::path::PathBuf,
}

impl RoutingStore for FileStore {
    fn load_backup(&self) -> Result<Option<RoutingBackup>, WinError> {
        match std::fs::read_to_string(&self.path) {
            Ok(s) => serde_json::from_str(&s).map_err(|e| WinError::Store(e.to_string())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(WinError::Store(e.to_string())),
        }
    }

    fn save_backup(&self, backup: Option<&RoutingBackup>) -> Result<(), WinError> {
        use std::io::Write;
        let json = serde_json::to_string(&backup).map_err(|e| WinError::Store(e.to_string()))?;
        let tmp = self.path.with_extension("tmp");
        let mut f = std::fs::File::create(&tmp).map_err(|e| WinError::Store(e.to_string()))?;
        f.write_all(json.as_bytes())
            .and_then(|_| f.sync_all())
            .map_err(|e| WinError::Store(e.to_string()))?;
        std::fs::rename(&tmp, &self.path).map_err(|e| WinError::Store(e.to_string()))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Default)]
    pub(crate) struct FakeControl {
        pub defaults: Mutex<HashMap<Role, String>>,
        pub fail_set: Mutex<Option<Role>>,
    }

    impl DefaultEndpointControl for FakeControl {
        fn get_default_capture(&self, role: Role) -> Result<Option<String>, WinError> {
            Ok(self.defaults.lock().unwrap().get(&role).cloned())
        }
        fn set_default_capture(&self, id: &str, role: Role) -> Result<(), WinError> {
            if *self.fail_set.lock().unwrap() == Some(role) {
                return Err(WinError::Os {
                    context: "SetDefaultEndpoint",
                    code: 0x8000_4005,
                });
            }
            self.defaults.lock().unwrap().insert(role, id.to_string());
            Ok(())
        }
    }

    #[derive(Default)]
    pub(crate) struct MemStore(pub Mutex<Option<RoutingBackup>>);

    impl RoutingStore for MemStore {
        fn load_backup(&self) -> Result<Option<RoutingBackup>, WinError> {
            Ok(self.0.lock().unwrap().clone())
        }
        fn save_backup(&self, b: Option<&RoutingBackup>) -> Result<(), WinError> {
            *self.0.lock().unwrap() = b.cloned();
            Ok(())
        }
    }

    fn fake() -> FakeControl {
        let c = FakeControl::default();
        c.defaults
            .lock()
            .unwrap()
            .insert(Role::Console, "headset-mic".into());
        c.defaults
            .lock()
            .unwrap()
            .insert(Role::Communications, "webcam-mic".into());
        c
    }

    #[test]
    fn fr14_enable_then_disable_restores_previous_defaults() {
        let (c, s) = (fake(), MemStore::default());
        let r = RouteAllApps::new(&c, &s);
        r.enable("cable-out").unwrap();
        assert!(r.is_active().unwrap());
        assert_eq!(
            c.get_default_capture(Role::Console).unwrap().as_deref(),
            Some("cable-out")
        );
        assert_eq!(
            c.get_default_capture(Role::Communications)
                .unwrap()
                .as_deref(),
            Some("cable-out")
        );
        r.disable().unwrap();
        assert!(!r.is_active().unwrap());
        assert_eq!(
            c.get_default_capture(Role::Console).unwrap().as_deref(),
            Some("headset-mic")
        );
        assert_eq!(
            c.get_default_capture(Role::Communications)
                .unwrap()
                .as_deref(),
            Some("webcam-mic")
        );
    }

    #[test]
    fn fr14_enabling_twice_keeps_the_original_backup() {
        let (c, s) = (fake(), MemStore::default());
        let r = RouteAllApps::new(&c, &s);
        r.enable("cable-out").unwrap();
        r.enable("cable-out").unwrap();
        r.disable().unwrap();
        assert_eq!(
            c.get_default_capture(Role::Console).unwrap().as_deref(),
            Some("headset-mic")
        );
    }

    #[test]
    fn nfr08_dirty_backup_is_restored_on_next_launch() {
        let (c, s) = (fake(), MemStore::default());
        RouteAllApps::new(&c, &s).enable("cable-out").unwrap();
        // "Crash": no disable. Next launch:
        let r = RouteAllApps::new(&c, &s);
        assert!(r.recover_after_crash().unwrap());
        assert_eq!(
            c.get_default_capture(Role::Console).unwrap().as_deref(),
            Some("headset-mic")
        );
        assert!(
            !r.recover_after_crash().unwrap(),
            "second launch has nothing to do"
        );
    }

    #[test]
    fn fr14_failure_rolls_back_and_clears_backup() {
        let (c, s) = (fake(), MemStore::default());
        *c.fail_set.lock().unwrap() = Some(Role::Communications);
        let r = RouteAllApps::new(&c, &s);
        assert!(r.enable("cable-out").is_err());
        assert_eq!(
            c.get_default_capture(Role::Console).unwrap().as_deref(),
            Some("headset-mic")
        );
        assert!(s.load_backup().unwrap().is_none());
    }

    #[test]
    fn fr14_unsupported_control_errors_without_touching_store() {
        let s = MemStore::default();
        let r = RouteAllApps::new(&UnsupportedControl, &s);
        assert!(matches!(r.enable("x"), Err(WinError::Unsupported(_))));
        assert!(s.load_backup().unwrap().is_none());
    }

    #[test]
    fn file_store_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let s = FileStore {
            path: dir.path().join("routing.json"),
        };
        assert!(s.load_backup().unwrap().is_none());
        let b = RoutingBackup {
            console: Some("a".into()),
            communications: None,
            dirty: true,
        };
        s.save_backup(Some(&b)).unwrap();
        assert_eq!(s.load_backup().unwrap(), Some(b));
        s.save_backup(None).unwrap();
        assert!(s.load_backup().unwrap().is_none());
    }
}
