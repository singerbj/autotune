//! Persisted settings (FR-20) in `%APPDATA%\<identifier>\config.json` with a
//! schema version and step-by-step migrations. Writes are atomic
//! (temp file + fsync + rename) because the routing backup must survive a
//! crash (NFR-08).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use specta::Type;
use tuner_dsp::TuningParams;
use tuner_win::{RoutingBackup, RoutingStore, WinError};

pub const SCHEMA_VERSION: u32 = 2;
pub const DEFAULT_HOTKEY: &str = "CommandOrControl+Alt+B";

/// A named tuning preset (FR-21).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    pub name: String,
    pub params: TuningParams,
}

/// Missing fields are filled from [`AppConfig::default`] by [`migrate`], so
/// the type itself has no serde defaults (keeps the TS type strict).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    pub schema_version: u32,
    pub params: TuningParams,
    /// Endpoint IDs; `None` = system default.
    pub capture_device: Option<String>,
    pub monitor_device: Option<String>,
    pub monitor_enabled: bool,
    pub monitor_volume: f32,
    /// Send the tuned voice into VB-Cable (the virtual mic).
    pub virtual_mic_enabled: bool,
    pub allow_exclusive: bool,
    pub allow_asio: bool,
    /// FR-14 toggle as the user last set it.
    pub route_all_apps: bool,
    /// Saved defaults while "Use for all apps" is active.
    pub routing_backup: Option<RoutingBackup>,
    pub wizard_completed: bool,
    pub launch_at_login: bool,
    pub start_engine_on_launch: bool,
    pub bypass_hotkey: String,
    pub auto_update: bool,
    pub presets: Vec<Preset>,
    /// Last acoustic loopback measurement, ms.
    pub measured_latency_ms: Option<f32>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            params: TuningParams::default(),
            capture_device: None,
            monitor_device: None,
            monitor_enabled: true,
            monitor_volume: 0.8,
            virtual_mic_enabled: true,
            allow_exclusive: true,
            allow_asio: true,
            route_all_apps: false,
            routing_backup: None,
            wizard_completed: false,
            launch_at_login: false,
            start_engine_on_launch: true,
            bypass_hotkey: DEFAULT_HOTKEY.into(),
            auto_update: true,
            presets: Vec::new(),
            measured_latency_ms: None,
        }
    }
}

/// Partial update from the UI (`set_config`). `None` leaves a field alone.
#[derive(Clone, Debug, Default, Deserialize, Type)]
#[serde(rename_all = "camelCase", default)]
pub struct ConfigPatch {
    pub capture_device: Option<Option<String>>,
    pub monitor_device: Option<Option<String>>,
    pub monitor_enabled: Option<bool>,
    pub monitor_volume: Option<f32>,
    pub virtual_mic_enabled: Option<bool>,
    pub allow_exclusive: Option<bool>,
    pub allow_asio: Option<bool>,
    pub start_engine_on_launch: Option<bool>,
    pub bypass_hotkey: Option<String>,
    pub auto_update: Option<bool>,
}

impl AppConfig {
    /// Apply a patch; returns true when audio devices/engine options changed
    /// (the engine must be rebuilt).
    pub fn apply(&mut self, p: ConfigPatch) -> bool {
        let before = (
            self.capture_device.clone(),
            self.monitor_device.clone(),
            self.virtual_mic_enabled,
            self.allow_exclusive,
            self.allow_asio,
        );
        if let Some(v) = p.capture_device {
            self.capture_device = v;
        }
        if let Some(v) = p.monitor_device {
            self.monitor_device = v;
        }
        if let Some(v) = p.monitor_enabled {
            self.monitor_enabled = v;
        }
        if let Some(v) = p.monitor_volume {
            self.monitor_volume = if v.is_finite() {
                v.clamp(0.0, 1.0)
            } else {
                0.8
            };
        }
        if let Some(v) = p.virtual_mic_enabled {
            self.virtual_mic_enabled = v;
        }
        if let Some(v) = p.allow_exclusive {
            self.allow_exclusive = v;
        }
        if let Some(v) = p.allow_asio {
            self.allow_asio = v;
        }
        if let Some(v) = p.start_engine_on_launch {
            self.start_engine_on_launch = v;
        }
        if let Some(v) = p.bypass_hotkey {
            self.bypass_hotkey = v;
        }
        if let Some(v) = p.auto_update {
            self.auto_update = v;
        }
        before
            != (
                self.capture_device.clone(),
                self.monitor_device.clone(),
                self.virtual_mic_enabled,
                self.allow_exclusive,
                self.allow_asio,
            )
    }
}

/// Recursively fill keys missing from `v` with those in `defaults`.
fn merge_defaults(v: &mut serde_json::Value, defaults: &serde_json::Value) {
    if let (Some(obj), Some(def)) = (v.as_object_mut(), defaults.as_object()) {
        for (k, dv) in def {
            match obj.get_mut(k) {
                Some(existing) => merge_defaults(existing, dv),
                None => {
                    obj.insert(k.clone(), dv.clone());
                }
            }
        }
    }
}

/// Upgrade raw JSON from any older schema to [`SCHEMA_VERSION`] and fill
/// fields added since with their defaults.
pub fn migrate(v: serde_json::Value) -> serde_json::Value {
    let mut v = migrate_schema(v);
    if let Ok(defaults) = serde_json::to_value(AppConfig::default()) {
        merge_defaults(&mut v, &defaults);
    }
    // Saved presets carry full parameter sets; give older ones the fields
    // added since (hard tune, formant, effects) at their neutral defaults.
    if let (Some(presets), Ok(params)) = (
        v.get_mut("presets").and_then(|p| p.as_array_mut()),
        serde_json::to_value(TuningParams::default()),
    ) {
        for preset in presets {
            if let Some(p) = preset.get_mut("params") {
                merge_defaults(p, &params);
            }
        }
    }
    v
}

fn migrate_schema(mut v: serde_json::Value) -> serde_json::Value {
    let version = v.get("schemaVersion").and_then(|x| x.as_u64()).unwrap_or(0);
    let Some(obj) = v.as_object_mut() else {
        return serde_json::json!({ "schemaVersion": SCHEMA_VERSION });
    };
    if version < 1 {
        // v0 (pre-release): flat "retuneSpeed" and "mic"/"headphones" keys.
        if let Some(mic) = obj.remove("mic") {
            obj.insert("captureDevice".into(), mic);
        }
        if let Some(hp) = obj.remove("headphones") {
            obj.insert("monitorDevice".into(), hp);
        }
        if let Some(rs) = obj.remove("retuneSpeed") {
            let params = obj.entry("params").or_insert_with(|| serde_json::json!({}));
            if let Some(p) = params.as_object_mut() {
                p.insert("retuneMs".into(), rs);
            }
        }
    }
    if version < 2 {
        // v2 added the virtual mic toggle; v1 always routed to VB-Cable.
        obj.entry("virtualMicEnabled")
            .or_insert(serde_json::Value::Bool(true));
    }
    obj.insert("schemaVersion".into(), SCHEMA_VERSION.into());
    v
}

/// Loads and saves [`AppConfig`].
#[derive(Debug)]
pub struct ConfigStore {
    path: PathBuf,
    pub config: AppConfig,
}

impl ConfigStore {
    /// Load from `path`; a missing file gives defaults, a corrupt file is
    /// moved aside to `config.json.bad` and replaced by defaults.
    pub fn load(path: PathBuf) -> Self {
        let config = match std::fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str::<serde_json::Value>(&text)
                .map(migrate)
                .and_then(serde_json::from_value::<AppConfig>)
            {
                Ok(mut c) => {
                    c.params = c.params.sanitized();
                    c
                }
                Err(e) => {
                    tracing::warn!(
                        "config at {} is invalid ({e}); using defaults",
                        path.display()
                    );
                    let _ = std::fs::rename(&path, path.with_extension("json.bad"));
                    AppConfig::default()
                }
            },
            Err(_) => AppConfig::default(),
        };
        Self { path, config }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn save(&self) -> std::io::Result<()> {
        use std::io::Write;
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_vec_pretty(&self.config)?;
        let tmp = self.path.with_extension("json.tmp");
        {
            let mut f = std::fs::File::create(&tmp)?;
            f.write_all(&json)?;
            f.sync_all()?;
        }
        std::fs::rename(&tmp, &self.path)
    }
}

/// The config store shared by the whole app.
#[derive(Debug)]
pub struct SharedConfig(std::sync::Mutex<ConfigStore>);

impl SharedConfig {
    pub fn new(store: ConfigStore) -> Self {
        Self(std::sync::Mutex::new(store))
    }

    /// Lock, ignoring poisoning (a panicked command must not brick the app).
    pub fn lock(&self) -> std::sync::MutexGuard<'_, ConfigStore> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// The routing backup lives in the config file (Architecture › Windows
/// default devices) and is saved immediately, not debounced.
impl RoutingStore for SharedConfig {
    fn load_backup(&self) -> Result<Option<RoutingBackup>, WinError> {
        Ok(self.lock().config.routing_backup.clone())
    }

    fn save_backup(&self, backup: Option<&RoutingBackup>) -> Result<(), WinError> {
        let mut store = self.lock();
        store.config.routing_backup = backup.cloned();
        store.save().map_err(|e| WinError::Store(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fr20_settings_survive_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let mut s = ConfigStore::load(path.clone());
        assert_eq!(s.config, AppConfig::default());
        s.config.params.key = 7;
        s.config.capture_device = Some("{mic}".into());
        s.config.presets.push(Preset {
            name: "Robot".into(),
            params: TuningParams {
                retune_ms: 0.0,
                ..Default::default()
            },
        });
        s.save().unwrap();
        let again = ConfigStore::load(path);
        assert_eq!(again.config, s.config);
    }

    #[test]
    fn fr20_migrates_v0_config() {
        let v0 = serde_json::json!({ "mic": "m1", "headphones": "h1", "retuneSpeed": 55.0 });
        let v = migrate(v0);
        let c: AppConfig = serde_json::from_value(v).unwrap();
        assert_eq!(c.schema_version, SCHEMA_VERSION);
        assert_eq!(c.capture_device.as_deref(), Some("m1"));
        assert_eq!(c.monitor_device.as_deref(), Some("h1"));
        assert_eq!(c.params.retune_ms, 55.0);
        assert!(c.virtual_mic_enabled);
    }

    #[test]
    fn fr20_fr25_old_params_and_presets_gain_neutral_effects() {
        let old_params = serde_json::json!({
            "key": 2, "scale": "major", "customMask": 4095, "retuneMs": 0.0,
            "humanize": 0.0, "voiceRange": "mid", "mix": 1.0,
            "gateThresholdDb": -60.0, "bypass": false
        });
        let v2 = serde_json::json!({
            "schemaVersion": 2,
            "params": old_params,
            "presets": [{ "name": "Robot", "params": old_params }],
        });
        let c: AppConfig = serde_json::from_value(migrate(v2)).unwrap();
        for p in [c.params, c.presets[0].params] {
            assert_eq!(p.key, 2);
            assert_eq!(p.retune_ms, 0.0);
            assert!(!p.hard_tune);
            assert_eq!(p.formant_semitones, 0.0);
            assert_eq!(p.fx, tuner_dsp::FxParams::default());
        }
    }

    #[test]
    fn fr20_migrates_v1_config_and_keeps_fields() {
        let v1 = serde_json::json!({ "schemaVersion": 1, "wizardCompleted": true, "monitorVolume": 0.3 });
        let c: AppConfig = serde_json::from_value(migrate(v1)).unwrap();
        assert!(c.wizard_completed);
        assert_eq!(c.monitor_volume, 0.3);
        assert!(c.virtual_mic_enabled);
    }

    #[test]
    fn corrupt_config_is_moved_aside() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(&path, "{ not json").unwrap();
        let s = ConfigStore::load(path.clone());
        assert_eq!(s.config, AppConfig::default());
        assert!(path.with_extension("json.bad").exists());
    }

    #[test]
    fn patch_reports_engine_relevant_changes() {
        let mut c = AppConfig::default();
        assert!(!c.apply(ConfigPatch {
            monitor_volume: Some(0.5),
            ..Default::default()
        }));
        assert!(c.apply(ConfigPatch {
            capture_device: Some(Some("x".into())),
            ..Default::default()
        }));
        assert!(c.apply(ConfigPatch {
            capture_device: Some(None),
            ..Default::default()
        }));
        assert_eq!(c.capture_device, None);
    }

    #[test]
    fn nfr08_routing_backup_is_persisted_through_the_config() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let store = SharedConfig::new(ConfigStore::load(path.clone()));
        let b = RoutingBackup {
            console: Some("mic".into()),
            communications: None,
            dirty: true,
        };
        store.save_backup(Some(&b)).unwrap();
        let reloaded = ConfigStore::load(path);
        assert_eq!(reloaded.config.routing_backup, Some(b));
    }
}
