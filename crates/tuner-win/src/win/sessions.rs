//! Audio session enumeration on an endpoint (FR-12 conflicts, FR-13 Discord).

use windows::core::{Interface, GUID, PWSTR};
use windows::Win32::Foundation::{CloseHandle, PROPERTYKEY};
use windows::Win32::Media::Audio::{
    AudioSessionStateActive, AudioSessionStateExpired, IAudioSessionControl2, IAudioSessionManager2,
};
use windows::Win32::System::Com::{CLSCTX_ALL, STGM_READ};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};

use super::{enumerator, os_err, pcwstr, wide, Com};
use crate::setup::{AudioSession, SessionState};
use crate::WinError;

fn process_name(pid: u32) -> String {
    if pid == 0 {
        return "System".into();
    }
    // SAFETY: plain query-only handle; closed below.
    let Ok(h) = (unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }) else {
        return format!("pid {pid}");
    };
    let mut buf = [0u16; 1024];
    let mut len = buf.len() as u32;
    // SAFETY: `buf` has `len` u16 slots; the handle is valid.
    let ok = unsafe {
        QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len)
    }
    .is_ok();
    // SAFETY: closing the handle we opened.
    let _ = unsafe { CloseHandle(h) };
    if !ok {
        return format!("pid {pid}");
    }
    let path = String::from_utf16_lossy(&buf[..len as usize]);
    path.rsplit('\\').next().unwrap_or(&path).to_string()
}

pub(crate) fn list(endpoint_id: &str) -> Result<Vec<AudioSession>, WinError> {
    let _com = Com::init();
    let en = enumerator()?;
    let w = wide(endpoint_id);
    // SAFETY: `w` is a null-terminated ID alive for the call.
    let dev = unsafe { en.GetDevice(pcwstr(&w)) }
        .map_err(|_| WinError::NotFound(endpoint_id.to_string()))?;
    // SAFETY: valid endpoint; COM initialised.
    let mgr: IAudioSessionManager2 = unsafe { dev.Activate(CLSCTX_ALL, None) }
        .map_err(|e| os_err("Activate(IAudioSessionManager2)", &e))?;
    // SAFETY: valid manager.
    let sessions =
        unsafe { mgr.GetSessionEnumerator() }.map_err(|e| os_err("GetSessionEnumerator", &e))?;
    // SAFETY: valid enumerator.
    let count = unsafe { sessions.GetCount() }.map_err(|e| os_err("GetCount", &e))?;
    let mut out = Vec::new();
    for i in 0..count {
        // SAFETY: `i < count`.
        let Ok(ctrl) = (unsafe { sessions.GetSession(i) }) else {
            continue;
        };
        let Ok(ctrl2) = ctrl.cast::<IAudioSessionControl2>() else {
            continue;
        };
        // SAFETY: valid session control.
        if unsafe { ctrl2.IsSystemSoundsSession() }.0 == 0 {
            continue; // S_OK means "this is the system sounds session"
        }
        // SAFETY: valid session control (multi-process sessions still return a pid).
        let pid = unsafe { ctrl2.GetProcessId() }.unwrap_or(0);
        // SAFETY: valid session control.
        let state = match unsafe { ctrl.GetState() } {
            Ok(s) if s == AudioSessionStateActive => SessionState::Active,
            Ok(s) if s == AudioSessionStateExpired => SessionState::Expired,
            _ => SessionState::Inactive,
        };
        out.push(AudioSession {
            pid,
            process_name: process_name(pid),
            state,
        });
    }
    Ok(out)
}

/// `{24dbb0fc-9311-4b3d-9cf0-18ff155639d4},1`: "Listen to this device".
const PKEY_LISTEN_ENABLED: PROPERTYKEY = PROPERTYKEY {
    fmtid: GUID::from_u128(0x24dbb0fc_9311_4b3d_9cf0_18ff155639d4),
    pid: 1,
};

pub(crate) fn listen_enabled(endpoint_id: &str) -> Option<bool> {
    let _com = Com::init();
    let en = enumerator().ok()?;
    let w = wide(endpoint_id);
    // SAFETY: valid ID string; returned objects are owned.
    let store = unsafe { en.GetDevice(pcwstr(&w)).ok()?.OpenPropertyStore(STGM_READ) }.ok()?;
    // SAFETY: valid store and key; the PROPVARIANT clears itself on drop.
    let v = unsafe { store.GetValue(&PKEY_LISTEN_ENABLED) }.ok()?;
    if v.is_empty() {
        return Some(false);
    }
    Some(matches!(
        v.to_string().to_ascii_lowercase().as_str(),
        "true" | "-1" | "1"
    ))
}
