//! The global tuning on/off hotkey (FR-10) via tauri-plugin-global-shortcut,
//! which uses `RegisterHotKey` on Windows (no keyboard hooks). Accelerators
//! are the plugin's syntax ("CommandOrControl+Alt+B", "Ctrl+Shift+F9"; keys
//! may also be DOM `KeyboardEvent.code` names such as "KeyB").
//!
//! The hotkey can change while the app runs: the new one is registered
//! before the old one is dropped, so an invalid or taken key leaves the
//! working one in place. At startup a configured key that can't be
//! registered falls back to [`DEFAULT_HOTKEY`].

use std::str::FromStr;

use serde::Serialize;
use specta::Type;
use tauri::AppHandle;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Shortcut};

use crate::config::DEFAULT_HOTKEY;

/// What the UI shows under the hotkey setting.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct HotkeyStatus {
    /// The registered accelerator; `None` if nothing could be registered.
    pub active: Option<String>,
    /// Why the configured hotkey isn't the active one.
    pub problem: Option<String>,
}

/// What is registered with the OS right now.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Registration {
    /// The registered shortcut, to unregister it when the setting changes.
    pub shortcut: Option<Shortcut>,
    pub status: HotkeyStatus,
}

impl Registration {
    fn ok(shortcut: Shortcut, accelerator: &str) -> Self {
        Self {
            shortcut: Some(shortcut),
            status: HotkeyStatus {
                active: Some(accelerator.trim().to_string()),
                problem: None,
            },
        }
    }
}

fn is_function_key(key: Code) -> bool {
    use Code::*;
    matches!(
        key,
        F1 | F2
            | F3
            | F4
            | F5
            | F6
            | F7
            | F8
            | F9
            | F10
            | F11
            | F12
            | F13
            | F14
            | F15
            | F16
            | F17
            | F18
            | F19
            | F20
            | F21
            | F22
            | F23
            | F24
    )
}

/// Parses and checks an accelerator. A key without modifiers is only
/// allowed for F1–F24, so the hotkey can't swallow normal typing.
pub fn parse(accelerator: &str) -> Result<Shortcut, String> {
    let text = accelerator.trim();
    if text.is_empty() {
        return Err("The hotkey can't be empty.".into());
    }
    let shortcut =
        Shortcut::from_str(text).map_err(|e| format!("\"{text}\" is not a valid hotkey: {e}"))?;
    if shortcut.mods.is_empty() && !is_function_key(shortcut.key) {
        return Err(format!(
            "\"{text}\" needs a modifier (Ctrl, Alt, Shift or Win), e.g. Ctrl+Alt+B."
        ));
    }
    Ok(shortcut)
}

fn try_register(app: &AppHandle, shortcut: Shortcut, accelerator: &str) -> Result<(), String> {
    app.global_shortcut().register(shortcut).map_err(|e| {
        format!(
            "Couldn't register \"{}\"; another app may be using it ({e}).",
            accelerator.trim()
        )
    })
}

/// Startup: registers `configured`, falling back to [`DEFAULT_HOTKEY`].
pub fn register(app: &AppHandle, configured: &str) -> Registration {
    let problem = match parse(configured) {
        Ok(shortcut) => match try_register(app, shortcut, configured) {
            Ok(()) => {
                tracing::info!("hotkey {} registered", configured.trim());
                return Registration::ok(shortcut, configured);
            }
            Err(e) => e,
        },
        Err(e) => e,
    };
    tracing::warn!("{problem}");
    let fallback = parse(DEFAULT_HOTKEY).ok().filter(|f| {
        // The configured key *is* the default and it failed: don't retry.
        parse(configured) != Ok(*f)
    });
    match fallback.map(|f| (f, try_register(app, f, DEFAULT_HOTKEY))) {
        Some((f, Ok(()))) => {
            tracing::warn!("using the default hotkey {DEFAULT_HOTKEY} instead");
            Registration {
                shortcut: Some(f),
                status: HotkeyStatus {
                    active: Some(DEFAULT_HOTKEY.into()),
                    problem: Some(format!("{problem} Using {DEFAULT_HOTKEY} instead.")),
                },
            }
        }
        Some((_, Err(e))) => Registration {
            shortcut: None,
            status: HotkeyStatus {
                active: None,
                problem: Some(format!("{problem} {e}")),
            },
        },
        None => Registration {
            shortcut: None,
            status: HotkeyStatus {
                active: None,
                problem: Some(problem),
            },
        },
    }
}

/// How to get from `current` to `shortcut`.
#[derive(Debug, PartialEq)]
enum Change {
    /// Already registered.
    Keep,
    /// Register the new one, then drop the old one (if any).
    Swap,
}

fn plan(current: &Registration, shortcut: Shortcut) -> Change {
    if current.shortcut == Some(shortcut) {
        Change::Keep
    } else {
        Change::Swap
    }
}

/// Switches to `configured` while the app runs. On error nothing changes:
/// the old hotkey stays registered and the caller keeps the old setting.
pub fn change(
    app: &AppHandle,
    current: &Registration,
    configured: &str,
) -> Result<Registration, String> {
    let shortcut = parse(configured)?;
    if plan(current, shortcut) == Change::Swap {
        try_register(app, shortcut, configured)?;
        if let Some(old) = current.shortcut {
            if let Err(e) = app.global_shortcut().unregister(old) {
                tracing::warn!("can't unregister the old hotkey: {e}");
            }
        }
        tracing::info!("hotkey changed to {}", configured.trim());
    }
    Ok(Registration::ok(shortcut, configured))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tauri_plugin_global_shortcut::Modifiers;

    fn parsed(s: &str) -> (Modifiers, Code) {
        let h = parse(s).unwrap_or_else(|e| panic!("{s}: {e}"));
        (h.mods, h.key)
    }

    #[test]
    fn fr10_accelerators() {
        let ctrl_or_cmd = if cfg!(target_os = "macos") {
            Modifiers::SUPER
        } else {
            Modifiers::CONTROL
        };
        assert_eq!(
            parsed(DEFAULT_HOTKEY),
            (ctrl_or_cmd | Modifiers::ALT, Code::KeyB)
        );
        assert_eq!(
            parsed(" Ctrl + Shift + T "),
            (Modifiers::CONTROL | Modifiers::SHIFT, Code::KeyT)
        );
        assert_eq!(parsed("Alt+KeyQ"), (Modifiers::ALT, Code::KeyQ));
        assert_eq!(parsed("Super+Digit7"), (Modifiers::SUPER, Code::Digit7));
        assert_eq!(parsed("Ctrl+Numpad1"), (Modifiers::CONTROL, Code::Numpad1));
        assert_eq!(parsed("F9"), (Modifiers::empty(), Code::F9));
        assert_eq!(parsed("Shift+F24").1, Code::F24);
    }

    #[test]
    fn fr10_invalid_accelerators() {
        for s in [
            "", "  ", "B", "Space", "Ctrl+Alt", "Hyper+B", "Ctrl+A+B", "Ctrl+Foo",
        ] {
            assert!(parse(s).is_err(), "{s:?} should be rejected");
        }
        assert!(parse("Hyper+B").unwrap_err().contains("Hyper+B"));
        assert!(parse("B").unwrap_err().contains("needs a modifier"));
    }

    #[test]
    fn fr10_plan_a_change() {
        let b = parse("Ctrl+Alt+B").unwrap();
        let current = Registration::ok(b, "Ctrl+Alt+B");
        assert_eq!(plan(&current, parse("ctrl+alt+b").unwrap()), Change::Keep);
        assert_eq!(plan(&current, parse("Ctrl+Alt+T").unwrap()), Change::Swap);
        assert_eq!(plan(&Registration::default(), b), Change::Swap);
    }
}
