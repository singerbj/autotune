//! Which Windows account this process runs as, compared with the person
//! signed in to the session.
//!
//! Plain UAC elevates the signed-in user themselves. Windows 11's
//! Administrator Protection instead runs elevated processes as a hidden admin
//! account, and "over the shoulder" elevation runs them as another admin. In
//! both cases `%APPDATA%`, `%LOCALAPPDATA%` and `HKCU` belong to that other
//! account, and WebView2 drops elevation to the signed-in user, who can't
//! write the other account's profile ("Microsoft Edge can't read and write to
//! its data directory").

use std::path::PathBuf;

use windows::core::{HSTRING, PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, LocalFree, HANDLE, HLOCAL};
use windows::Win32::Security::Authorization::ConvertSidToStringSidW;
use windows::Win32::Security::{
    GetTokenInformation, LookupAccountNameW, TokenElevation, TokenElevationType,
    TokenElevationTypeFull, TokenLinkedToken, TokenUser, PSID, SID_NAME_USE, TOKEN_ELEVATION,
    TOKEN_ELEVATION_TYPE, TOKEN_LINKED_TOKEN, TOKEN_QUERY, TOKEN_USER,
};
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};
use windows::Win32::System::RemoteDesktop::{
    WTSDomainName, WTSFreeMemory, WTSQuerySessionInformationW, WTSUserName,
    WTS_CURRENT_SERVER_HANDLE, WTS_CURRENT_SESSION, WTS_INFO_CLASS,
};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::Win32::UI::Shell::{
    FOLDERID_LocalAppData, FOLDERID_RoamingAppData, SHGetKnownFolderPath, KF_FLAG_DEFAULT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, MessageBoxW, MB_ICONERROR, MB_OK, SM_REMOTESESSION,
};

use crate::SignedInUser;

/// Closes a token handle on drop.
struct Token(HANDLE);

impl Drop for Token {
    fn drop(&mut self) {
        // SAFETY: we own the handle and close it once.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

fn process_token() -> Option<Token> {
    let mut token = HANDLE::default();
    // SAFETY: the pseudo-handle from GetCurrentProcess is always valid.
    unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) }.ok()?;
    Some(Token(token))
}

/// Reads a fixed-size token information struct.
fn token_info<T: Default>(
    token: &Token,
    class: windows::Win32::Security::TOKEN_INFORMATION_CLASS,
) -> Option<T> {
    let mut out = T::default();
    let mut len = 0u32;
    // SAFETY: `out` is a writable `T` of the size we pass.
    unsafe {
        GetTokenInformation(
            token.0,
            class,
            Some(&mut out as *mut T as *mut _),
            core::mem::size_of::<T>() as u32,
            &mut len,
        )
    }
    .ok()?;
    Some(out)
}

fn sid_string(sid: PSID) -> Option<String> {
    let mut s = PWSTR::null();
    // SAFETY: `sid` is a valid SID; the returned string is LocalAlloc'd and freed once.
    unsafe {
        ConvertSidToStringSidW(sid, &mut s).ok()?;
        let out = s.to_string().ok();
        let _ = LocalFree(Some(HLOCAL(s.0 as *mut _)));
        out
    }
}

fn token_sid(token: &Token) -> Option<String> {
    let mut len = 0u32;
    // SAFETY: size query only; it fails with ERROR_INSUFFICIENT_BUFFER and sets `len`.
    let _ = unsafe { GetTokenInformation(token.0, TokenUser, None, 0, &mut len) };
    let mut buf = vec![0u64; (len as usize).div_ceil(8)];
    // SAFETY: `buf` holds `len` bytes, 8-byte aligned for TOKEN_USER.
    unsafe {
        GetTokenInformation(
            token.0,
            TokenUser,
            Some(buf.as_mut_ptr() as *mut _),
            len,
            &mut len,
        )
        .ok()?;
        let user = &*(buf.as_ptr() as *const TOKEN_USER);
        sid_string(user.User.Sid)
    }
}

pub(crate) fn is_elevated() -> bool {
    process_token()
        .and_then(|t| token_info::<TOKEN_ELEVATION>(&t, TokenElevation))
        .is_some_and(|e| e.TokenIsElevated != 0)
}

/// `DOMAIN\user` signed in to this process's Windows session (none in session 0).
fn session_user_name() -> Option<String> {
    fn query(class: WTS_INFO_CLASS) -> Option<String> {
        let mut buf = PWSTR::null();
        let mut len = 0u32;
        // SAFETY: WTS allocates `buf`, which we free once with WTSFreeMemory.
        unsafe {
            WTSQuerySessionInformationW(
                Some(WTS_CURRENT_SERVER_HANDLE),
                WTS_CURRENT_SESSION,
                class,
                &mut buf,
                &mut len,
            )
            .ok()?;
            let out = buf.to_string().ok();
            WTSFreeMemory(buf.0 as *mut _);
            out.filter(|s| !s.is_empty())
        }
    }
    let user = query(WTSUserName)?;
    Some(match query(WTSDomainName) {
        Some(domain) => format!(r"{domain}\{user}"),
        None => user,
    })
}

/// SID string of an account name such as `DOMAIN\user`.
fn account_sid(name: &str) -> Option<String> {
    let name = HSTRING::from(name);
    let (mut sid_len, mut domain_len) = (0u32, 0u32);
    let mut kind = SID_NAME_USE::default();
    // SAFETY: size query only; fails and fills in the two lengths.
    let _ = unsafe {
        LookupAccountNameW(
            PCWSTR::null(),
            &name,
            None,
            &mut sid_len,
            None,
            &mut domain_len,
            &mut kind,
        )
    };
    if sid_len == 0 {
        return None;
    }
    let mut sid = vec![0u64; (sid_len as usize).div_ceil(8)];
    let mut domain = vec![0u16; domain_len.max(1) as usize];
    // SAFETY: both buffers have the sizes LookupAccountNameW asked for.
    unsafe {
        LookupAccountNameW(
            PCWSTR::null(),
            &name,
            Some(PSID(sid.as_mut_ptr() as *mut _)),
            &mut sid_len,
            Some(PWSTR(domain.as_mut_ptr())),
            &mut domain_len,
            &mut kind,
        )
        .ok()?;
    }
    sid_string(PSID(sid.as_mut_ptr() as *mut _))
}

fn known_folder(id: &windows::core::GUID, token: Option<&Token>) -> Option<PathBuf> {
    // SAFETY: the returned string is CoTaskMemAlloc'd and freed once.
    unsafe {
        let p = SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, token.map(|t| t.0)).ok()?;
        let out = p.to_string().ok().map(PathBuf::from);
        CoTaskMemFree(Some(p.0 as *const _));
        out
    }
}

/// The profile folder of the account `sid`, from the registry.
fn profile_dir(sid: &str) -> Option<PathBuf> {
    let key = HSTRING::from(format!(
        r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\ProfileList\{sid}"
    ));
    let value = HSTRING::from("ProfileImagePath");
    let mut buf = vec![0u16; 1024];
    let mut len = (buf.len() * 2) as u32;
    // SAFETY: `buf` holds `len` bytes; REG_EXPAND_SZ values are expanded.
    unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            &key,
            &value,
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr() as *mut _),
            Some(&mut len),
        )
    }
    .ok()
    .ok()?;
    let chars = (len as usize / 2).min(buf.len());
    let s = String::from_utf16_lossy(&buf[..chars]);
    let s = s.trim_end_matches('\0');
    (!s.is_empty()).then(|| PathBuf::from(s))
}

fn from_profile(sid: String) -> Option<SignedInUser> {
    let profile = profile_dir(&sid)?;
    Some(SignedInUser {
        roaming_app_data: profile.join(r"AppData\Roaming"),
        local_app_data: profile.join(r"AppData\Local"),
        sid,
    })
}

pub(crate) fn other_signed_in_user() -> Option<SignedInUser> {
    if !is_elevated() {
        return None;
    }
    let token = process_token()?;
    let me = token_sid(&token)?;

    // Some builds make the signed-in user the elevated token's linked token.
    let kind = token_info::<TOKEN_ELEVATION_TYPE>(&token, TokenElevationType);
    if kind == Some(TokenElevationTypeFull) {
        if let Some(linked) = token_info::<TOKEN_LINKED_TOKEN>(&token, TokenLinkedToken) {
            let linked = Token(linked.LinkedToken);
            if let Some(sid) = token_sid(&linked).filter(|s| *s != me) {
                let roaming = known_folder(&FOLDERID_RoamingAppData, Some(&linked));
                let local = known_folder(&FOLDERID_LocalAppData, Some(&linked));
                return match (roaming, local) {
                    (Some(roaming_app_data), Some(local_app_data)) => Some(SignedInUser {
                        sid,
                        roaming_app_data,
                        local_app_data,
                    }),
                    // The linked token may be too weak for the shell.
                    _ => from_profile(sid),
                };
            }
        }
    }

    // Otherwise, the session's user always is (as with over-the-shoulder elevation).
    let sid = account_sid(&session_user_name()?)?;
    if sid == me {
        return None;
    }
    from_profile(sid)
}

pub(crate) fn error_box(title: &str, text: &str) {
    let (title, text) = (HSTRING::from(title), HSTRING::from(text));
    // SAFETY: both strings outlive the call; no owner window.
    unsafe { MessageBoxW(None, &text, &title, MB_OK | MB_ICONERROR) };
}

pub(crate) fn is_remote_session() -> bool {
    // SAFETY: plain query with no pointers.
    unsafe { GetSystemMetrics(SM_REMOTESESSION) != 0 }
}
