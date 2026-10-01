//! Keeping Windows' default devices off VB-Cable (ADR 0011).
//!
//! Windows often makes a newly installed cable the default playback device,
//! so every app's sound goes into "CABLE Input" and the user hears nothing.
//! Removing the cable while it is a default leaves Windows to pick a
//! replacement, often the wrong one. This module decides what to change;
//! callers apply it through [`DefaultEndpointControl`].
//!
//! * Before installing: [`DefaultsSnapshot::read`].
//! * Once the cable is present: [`defaults_to_restore`] puts back every role
//!   the install moved onto the cable.
//! * Before removing, or when the cable took over playback:
//!   [`release_plan`] moves those roles to a real device.

use serde::{Deserialize, Serialize};

use crate::routing::{DefaultEndpointControl, Flow, Role, ALL_ROLES};
use crate::setup::EndpointSummary;
use crate::WinError;

/// One default endpoint: `id` is the default `flow` device for `role`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DefaultEntry {
    pub flow: Flow,
    pub role: Role,
    pub id: String,
}

/// Every default endpoint at one moment.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DefaultsSnapshot {
    pub entries: Vec<DefaultEntry>,
}

impl DefaultsSnapshot {
    /// Read the defaults for both flows and every role. Roles with no
    /// default (no device of that flow) are left out.
    pub fn read(control: &dyn DefaultEndpointControl) -> Result<Self, WinError> {
        let mut entries = Vec::new();
        for flow in [Flow::Render, Flow::Capture] {
            for role in ALL_ROLES {
                if let Some(id) = control.get_default(flow, role)? {
                    entries.push(DefaultEntry { flow, role, id });
                }
            }
        }
        Ok(Self { entries })
    }

    pub fn get(&self, flow: Flow, role: Role) -> Option<&str> {
        self.entries
            .iter()
            .find(|e| e.flow == flow && e.role == role)
            .map(|e| e.id.as_str())
    }
}

/// True when `id` is one of VB-Cable's endpoints.
pub fn is_cable(endpoints: &[EndpointSummary], id: &str) -> bool {
    endpoints.iter().any(|e| e.id == id && e.is_vb_cable)
}

fn flow_of(e: &EndpointSummary) -> Flow {
    if e.is_capture {
        Flow::Capture
    } else {
        Flow::Render
    }
}

/// The roles a VB-Cable install moved onto the cable: the cable is the
/// default now, and a real device was before. Roles the user had already
/// pointed at the cable stay as they are.
pub fn defaults_to_restore(
    before: &DefaultsSnapshot,
    now: &DefaultsSnapshot,
    endpoints: &[EndpointSummary],
) -> Vec<DefaultEntry> {
    now.entries
        .iter()
        .filter(|e| is_cable(endpoints, &e.id))
        .filter_map(|e| {
            let prev = before.get(e.flow, e.role)?;
            let still_there = endpoints
                .iter()
                .any(|d| d.id == prev && flow_of(d) == e.flow);
            (!is_cable(endpoints, prev) && still_there).then(|| DefaultEntry {
                flow: e.flow,
                role: e.role,
                id: prev.to_string(),
            })
        })
        .collect()
}

/// Where to move each default of `flows` that is a VB-Cable endpoint: the
/// real device another role of the same flow already uses, else the first
/// real device of that flow. Roles with no real device to move to are left
/// out (Windows keeps the cable).
pub fn release_plan(
    now: &DefaultsSnapshot,
    endpoints: &[EndpointSummary],
    flows: &[Flow],
) -> Vec<DefaultEntry> {
    let real = |flow: Flow, id: &str| {
        endpoints
            .iter()
            .any(|d| d.id == id && !d.is_vb_cable && flow_of(d) == flow)
    };
    now.entries
        .iter()
        .filter(|e| flows.contains(&e.flow) && is_cable(endpoints, &e.id))
        .filter_map(|e| {
            let sibling = ALL_ROLES
                .iter()
                .filter_map(|r| now.get(e.flow, *r))
                .find(|id| real(e.flow, id));
            let first = endpoints
                .iter()
                .find(|d| !d.is_vb_cable && flow_of(d) == e.flow)
                .map(|d| d.id.as_str());
            sibling.or(first).map(|id| DefaultEntry {
                flow: e.flow,
                role: e.role,
                id: id.to_string(),
            })
        })
        .collect()
}

/// True when any playback role defaults to CABLE Input, so apps that play
/// to the default device are silent.
pub fn playback_on_cable(now: &DefaultsSnapshot, endpoints: &[EndpointSummary]) -> bool {
    now.entries
        .iter()
        .any(|e| e.flow == Flow::Render && is_cable(endpoints, &e.id))
}

/// Apply every entry; keeps going after a failure and returns the first error.
pub fn apply(
    control: &dyn DefaultEndpointControl,
    entries: &[DefaultEntry],
) -> Result<(), WinError> {
    let mut first_err = None;
    for e in entries {
        if let Err(err) = control.set_default(&e.id, e.role) {
            first_err.get_or_insert(err);
        }
    }
    first_err.map_or(Ok(()), Err)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// Defaults per (flow, role); the flow of an ID comes from `endpoints`.
    pub(crate) struct FakeOs {
        pub endpoints: Vec<EndpointSummary>,
        pub defaults: Mutex<HashMap<(Flow, Role), String>>,
    }

    impl FakeOs {
        pub(crate) fn new(endpoints: Vec<EndpointSummary>) -> Self {
            Self {
                endpoints,
                defaults: Mutex::new(HashMap::new()),
            }
        }
        pub(crate) fn set_all(&self, flow: Flow, id: &str) {
            for r in ALL_ROLES {
                self.defaults.lock().unwrap().insert((flow, r), id.into());
            }
        }
        pub(crate) fn get(&self, flow: Flow, role: Role) -> Option<String> {
            self.defaults.lock().unwrap().get(&(flow, role)).cloned()
        }
    }

    impl DefaultEndpointControl for FakeOs {
        fn get_default_capture(&self, role: Role) -> Result<Option<String>, WinError> {
            Ok(self.get(Flow::Capture, role))
        }
        fn set_default_capture(&self, id: &str, role: Role) -> Result<(), WinError> {
            let flow = self
                .endpoints
                .iter()
                .find(|e| e.id == id)
                .map(flow_of)
                .ok_or_else(|| WinError::NotFound(id.into()))?;
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

    pub(crate) fn ep(id: &str, capture: bool, cable: bool) -> EndpointSummary {
        EndpointSummary {
            id: id.into(),
            name: id.into(),
            is_capture: capture,
            is_bluetooth: false,
            is_vb_cable: cable,
        }
    }

    pub(crate) fn endpoints() -> Vec<EndpointSummary> {
        vec![
            ep("headset-mic", true, false),
            ep("headphones", false, false),
            ep("speakers", false, false),
            ep("cable-in", false, true),
            ep("cable-out", true, true),
        ]
    }

    #[test]
    fn fr15_install_takeover_of_playback_is_undone() {
        let os = FakeOs::new(endpoints());
        os.set_all(Flow::Render, "headphones");
        os.set_all(Flow::Capture, "headset-mic");
        let before = DefaultsSnapshot::read(&os).unwrap();
        // Windows hands the new cable the speakers (all roles) and the mic
        // (communications only).
        os.set_all(Flow::Render, "cable-in");
        os.defaults
            .lock()
            .unwrap()
            .insert((Flow::Capture, Role::Communications), "cable-out".into());
        let now = DefaultsSnapshot::read(&os).unwrap();
        assert!(playback_on_cable(&now, &os.endpoints));

        let fix = defaults_to_restore(&before, &now, &os.endpoints);
        assert_eq!(fix.len(), 4);
        apply(&os, &fix).unwrap();
        for r in ALL_ROLES {
            assert_eq!(os.get(Flow::Render, r).as_deref(), Some("headphones"));
            assert_eq!(os.get(Flow::Capture, r).as_deref(), Some("headset-mic"));
        }
        let after = DefaultsSnapshot::read(&os).unwrap();
        assert!(!playback_on_cable(&after, &os.endpoints));
    }

    #[test]
    fn fr15_cable_defaults_the_user_chose_are_kept() {
        let os = FakeOs::new(endpoints());
        os.set_all(Flow::Capture, "cable-out");
        let before = DefaultsSnapshot::read(&os).unwrap();
        let now = before.clone();
        assert!(defaults_to_restore(&before, &now, &os.endpoints).is_empty());
    }

    #[test]
    fn fr15_previous_default_that_is_gone_is_not_restored() {
        let mut eps = endpoints();
        let os = FakeOs::new(eps.clone());
        os.set_all(Flow::Render, "usb-dac");
        let before = DefaultsSnapshot::read(&os).unwrap();
        os.set_all(Flow::Render, "cable-in");
        let now = DefaultsSnapshot::read(&os).unwrap();
        eps.retain(|e| e.id != "usb-dac");
        assert!(defaults_to_restore(&before, &now, &eps).is_empty());
    }

    #[test]
    fn release_moves_cable_defaults_to_the_sibling_role_device() {
        let os = FakeOs::new(endpoints());
        os.set_all(Flow::Render, "speakers");
        os.defaults
            .lock()
            .unwrap()
            .insert((Flow::Render, Role::Communications), "cable-in".into());
        os.set_all(Flow::Capture, "cable-out");
        let now = DefaultsSnapshot::read(&os).unwrap();

        let plan = release_plan(&now, &os.endpoints, &[Flow::Render, Flow::Capture]);
        apply(&os, &plan).unwrap();
        assert_eq!(
            os.get(Flow::Render, Role::Communications).as_deref(),
            Some("speakers"),
            "the device the other playback roles use"
        );
        for r in ALL_ROLES {
            assert_eq!(
                os.get(Flow::Capture, r).as_deref(),
                Some("headset-mic"),
                "no real mic was default, so the first real one"
            );
        }
    }

    #[test]
    fn release_only_touches_the_requested_flows() {
        let os = FakeOs::new(endpoints());
        os.set_all(Flow::Render, "cable-in");
        os.set_all(Flow::Capture, "cable-out");
        let now = DefaultsSnapshot::read(&os).unwrap();
        let plan = release_plan(&now, &os.endpoints, &[Flow::Render]);
        assert_eq!(plan.len(), 3);
        assert!(plan.iter().all(|e| e.flow == Flow::Render));
    }

    #[test]
    fn release_without_a_real_device_leaves_the_cable() {
        let eps = vec![ep("cable-in", false, true), ep("cable-out", true, true)];
        let os = FakeOs::new(eps);
        os.set_all(Flow::Render, "cable-in");
        let now = DefaultsSnapshot::read(&os).unwrap();
        assert!(release_plan(&now, &os.endpoints, &[Flow::Render]).is_empty());
    }

    #[test]
    fn apply_keeps_going_after_a_failure() {
        let os = FakeOs::new(endpoints());
        let entries = vec![
            DefaultEntry {
                flow: Flow::Render,
                role: Role::Console,
                id: "missing".into(),
            },
            DefaultEntry {
                flow: Flow::Render,
                role: Role::Multimedia,
                id: "speakers".into(),
            },
        ];
        assert!(apply(&os, &entries).is_err());
        assert_eq!(
            os.get(Flow::Render, Role::Multimedia).as_deref(),
            Some("speakers")
        );
    }

    #[test]
    fn snapshot_roundtrips_as_json() {
        let os = FakeOs::new(endpoints());
        os.set_all(Flow::Render, "speakers");
        let s = DefaultsSnapshot::read(&os).unwrap();
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains(r#""flow":"render""#));
        assert_eq!(serde_json::from_str::<DefaultsSnapshot>(&json).unwrap(), s);
    }
}
