//! "Run as administrator" for the VB-Cable repair helper (ADR 0011).

use std::path::Path;

use windows::core::w;
use windows::Win32::Foundation::{CloseHandle, ERROR_CANCELLED, WAIT_OBJECT_0};
use windows::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject, INFINITE};
use windows::Win32::UI::Shell::{
    ShellExecuteExW, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
};
use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;

use super::{os_err, pcwstr, wide};
use crate::WinError;

/// Start `program args` elevated (one UAC prompt), wait for it to exit and
/// return its exit code. A declined prompt is [`WinError::Cancelled`].
pub(crate) fn run_elevated(program: &Path, args: &str) -> Result<u32, WinError> {
    let file = wide(&program.to_string_lossy());
    let params = wide(args);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC,
        lpVerb: w!("runas"),
        lpFile: pcwstr(&file),
        lpParameters: pcwstr(&params),
        nShow: SW_HIDE.0,
        ..Default::default()
    };
    // SAFETY: `info` and the strings it points to outlive the call.
    if let Err(e) = unsafe { ShellExecuteExW(&mut info) } {
        return Err(if e.code() == ERROR_CANCELLED.to_hresult() {
            WinError::Cancelled
        } else {
            os_err("ShellExecuteExW", &e)
        });
    }
    let process = info.hProcess;
    if process.is_invalid() {
        return Err(WinError::Unsupported("waiting for the elevated helper"));
    }
    // SAFETY: we own `process` (SEE_MASK_NOCLOSEPROCESS) and close it below.
    let waited = unsafe { WaitForSingleObject(process, INFINITE) };
    let mut code = 1u32;
    // SAFETY: valid process handle; `code` is a valid out pointer.
    let got = unsafe { GetExitCodeProcess(process, &mut code) };
    // SAFETY: closing the handle ShellExecuteExW gave us.
    let _ = unsafe { CloseHandle(process) };
    if waited != WAIT_OBJECT_0 {
        return Err(WinError::Unsupported("waiting for the elevated helper"));
    }
    got.map_err(|e| os_err("GetExitCodeProcess", &e))?;
    Ok(code)
}
