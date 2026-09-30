//! Setup check evaluation (FR-12, FR-13). Pure logic over data gathered by
//! the platform layer, so it is fully unit-tested on any OS.

use serde::{Deserialize, Serialize};

/// Minimal endpoint description needed by the setup check.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct EndpointSummary {
    pub id: String,
    pub name: String,
    pub is_capture: bool,
    pub is_bluetooth: bool,
    pub is_vb_cable: bool,
}

/// A VB-Cable endpoint that exists but Windows won't let apps open.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct InactiveCable {
    pub name: String,
    pub is_capture: bool,
    /// Turned off in Sound settings (otherwise reported unplugged).
    pub disabled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum SessionState {
    Active,
    Inactive,
    Expired,
}

/// One WASAPI session on an endpoint.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct AudioSession {
    pub pid: u32,
    pub process_name: String,
    pub state: SessionState,
}

/// Everything the wizard and the setup panel show.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SetupReport {
    pub vb_cable_installed: bool,
    pub cable_input_id: Option<String>,
    pub cable_output_id: Option<String>,
    /// Why VB-Cable looks missing: its endpoints exist but are disabled or
    /// unplugged (only for sides with no active endpoint).
    pub inactive_cables: Vec<InactiveCable>,
    /// Other apps already sending audio into CABLE Input (OBS, music…).
    pub cable_conflicts: Vec<String>,
    /// A Discord process has a capture session on CABLE Output.
    pub discord_detected: bool,
    /// …and it is currently active (Discord is listening right now).
    pub discord_active: bool,
    /// Selected mic or headphones are Bluetooth (high latency, not supported).
    pub bluetooth_warning: bool,
    /// "Listen to this device" is on for the selected mic (doubled voice).
    pub sidetone_warning: bool,
    /// Running over Remote Desktop, where the PC's audio devices are hidden.
    pub remote_session: bool,
}

/// True for Discord's stable, PTB, Canary and development builds.
pub fn is_discord_process(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    let n = n.rsplit(['\\', '/']).next().unwrap_or(&n);
    n.starts_with("discord") && n.ends_with(".exe")
}

pub struct SetupInputs<'a> {
    pub endpoints: &'a [EndpointSummary],
    pub inactive_cables: &'a [InactiveCable],
    pub selected_mic: Option<&'a str>,
    pub selected_headphones: Option<&'a str>,
    pub cable_input_sessions: &'a [AudioSession],
    pub cable_output_sessions: &'a [AudioSession],
    pub own_pid: u32,
    pub listen_enabled: Option<bool>,
    pub remote_session: bool,
}

pub fn evaluate_setup(i: &SetupInputs<'_>) -> SetupReport {
    let cable_in = i.endpoints.iter().find(|e| e.is_vb_cable && !e.is_capture);
    let cable_out = i.endpoints.iter().find(|e| e.is_vb_cable && e.is_capture);

    let mut conflicts: Vec<String> = i
        .cable_input_sessions
        .iter()
        .filter(|s| s.pid != 0 && s.pid != i.own_pid && s.state != SessionState::Expired)
        .map(|s| s.process_name.clone())
        .collect();
    conflicts.sort();
    conflicts.dedup();

    let discord: Vec<&AudioSession> = i
        .cable_output_sessions
        .iter()
        .filter(|s| is_discord_process(&s.process_name) && s.state != SessionState::Expired)
        .collect();

    let is_bt = |id: Option<&str>| {
        id.and_then(|id| i.endpoints.iter().find(|e| e.id == id))
            .is_some_and(|e| e.is_bluetooth)
    };

    SetupReport {
        vb_cable_installed: cable_in.is_some() && cable_out.is_some(),
        cable_input_id: cable_in.map(|e| e.id.clone()),
        cable_output_id: cable_out.map(|e| e.id.clone()),
        inactive_cables: i
            .inactive_cables
            .iter()
            .filter(|c| {
                if c.is_capture {
                    cable_out.is_none()
                } else {
                    cable_in.is_none()
                }
            })
            .cloned()
            .collect(),
        cable_conflicts: conflicts,
        discord_detected: !discord.is_empty(),
        discord_active: discord.iter().any(|s| s.state == SessionState::Active),
        bluetooth_warning: is_bt(i.selected_mic) || is_bt(i.selected_headphones),
        sidetone_warning: i.listen_enabled == Some(true),
        remote_session: i.remote_session,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ep(id: &str, cap: bool, bt: bool, cable: bool) -> EndpointSummary {
        EndpointSummary {
            id: id.into(),
            name: id.into(),
            is_capture: cap,
            is_bluetooth: bt,
            is_vb_cable: cable,
        }
    }

    fn sess(pid: u32, name: &str, state: SessionState) -> AudioSession {
        AudioSession {
            pid,
            process_name: name.into(),
            state,
        }
    }

    fn endpoints() -> Vec<EndpointSummary> {
        vec![
            ep("mic", true, false, false),
            ep("hp", false, false, false),
            ep("bt-hp", false, true, false),
            ep("cin", false, false, true),
            ep("cout", true, false, true),
        ]
    }

    fn inputs<'a>(
        eps: &'a [EndpointSummary],
        cin: &'a [AudioSession],
        cout: &'a [AudioSession],
    ) -> SetupInputs<'a> {
        SetupInputs {
            endpoints: eps,
            inactive_cables: &[],
            selected_mic: Some("mic"),
            selected_headphones: Some("hp"),
            cable_input_sessions: cin,
            cable_output_sessions: cout,
            own_pid: 42,
            listen_enabled: Some(false),
            remote_session: false,
        }
    }

    #[test]
    fn fr12_detects_vb_cable_and_ids() {
        let eps = endpoints();
        let r = evaluate_setup(&inputs(&eps, &[], &[]));
        assert!(r.vb_cable_installed);
        assert_eq!(r.cable_input_id.as_deref(), Some("cin"));
        assert_eq!(r.cable_output_id.as_deref(), Some("cout"));
        let no_cable: Vec<_> = eps.into_iter().filter(|e| !e.is_vb_cable).collect();
        assert!(!evaluate_setup(&inputs(&no_cable, &[], &[])).vb_cable_installed);
    }

    #[test]
    fn fr12_reports_inactive_cable_sides_only_when_missing() {
        let inactive = |name: &str, cap: bool| InactiveCable {
            name: name.into(),
            is_capture: cap,
            disabled: true,
        };
        let stale = [inactive("old in", false), inactive("old out", true)];
        let eps = endpoints();
        let mut i = inputs(&eps, &[], &[]);
        i.inactive_cables = &stale;
        assert!(evaluate_setup(&i).inactive_cables.is_empty());

        // CABLE Output disabled in Sound settings: only it is reported.
        let eps: Vec<_> = eps.into_iter().filter(|e| e.id != "cout").collect();
        let off = [inactive("CABLE Output (VB-Audio Virtual Cable)", true)];
        let mut i = inputs(&eps, &[], &[]);
        i.inactive_cables = &off;
        let r = evaluate_setup(&i);
        assert!(!r.vb_cable_installed);
        assert_eq!(r.inactive_cables, off.to_vec());
    }

    #[test]
    fn fr12_cable_conflicts_exclude_self_and_expired() {
        let eps = endpoints();
        let cin = [
            sess(42, "tunedup.exe", SessionState::Active),
            sess(7, "obs64.exe", SessionState::Active),
            sess(8, "obs64.exe", SessionState::Inactive),
            sess(9, "spotify.exe", SessionState::Expired),
            sess(0, "system", SessionState::Active),
        ];
        let r = evaluate_setup(&inputs(&eps, &cin, &[]));
        assert_eq!(r.cable_conflicts, vec!["obs64.exe".to_string()]);
    }

    #[test]
    fn fr13_discord_session_detection() {
        let eps = endpoints();
        let cout = [sess(
            5,
            "C:\\Users\\a\\AppData\\Local\\Discord\\app-1.0\\Discord.exe",
            SessionState::Inactive,
        )];
        let r = evaluate_setup(&inputs(&eps, &[], &cout));
        assert!(r.discord_detected);
        assert!(!r.discord_active);
        let cout = [sess(5, "DiscordCanary.exe", SessionState::Active)];
        let r = evaluate_setup(&inputs(&eps, &[], &cout));
        assert!(r.discord_active);
        let cout = [sess(5, "chrome.exe", SessionState::Active)];
        assert!(!evaluate_setup(&inputs(&eps, &[], &cout)).discord_detected);
    }

    #[test]
    fn fr12_bluetooth_and_sidetone_warnings() {
        let eps = endpoints();
        let mut i = inputs(&eps, &[], &[]);
        assert!(!evaluate_setup(&i).bluetooth_warning);
        i.selected_headphones = Some("bt-hp");
        assert!(evaluate_setup(&i).bluetooth_warning);
        i.listen_enabled = Some(true);
        assert!(evaluate_setup(&i).sidetone_warning);
    }

    #[test]
    fn discord_names() {
        assert!(is_discord_process("Discord.exe"));
        assert!(is_discord_process("discordptb.exe"));
        assert!(!is_discord_process("notdiscord.exe"));
        assert!(!is_discord_process("Discord"));
    }
}
