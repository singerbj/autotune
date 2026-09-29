//! Windows implementations.

pub(crate) mod notify;
pub(crate) mod policy;
pub(crate) mod sessions;

use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Media::Audio::{IMMDeviceEnumerator, MMDeviceEnumerator};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
    COINIT_MULTITHREADED,
};

use crate::WinError;

pub(crate) struct Com {
    owned: bool,
    _not_send: core::marker::PhantomData<*const ()>,
}

impl Com {
    pub(crate) fn init() -> Self {
        // SAFETY: no preconditions; failure (already STA) leaves COM usable.
        let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        Self {
            owned: hr.is_ok(),
            _not_send: core::marker::PhantomData,
        }
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        if self.owned {
            // SAFETY: balances the successful CoInitializeEx on this thread.
            unsafe { CoUninitialize() };
        }
    }
}

pub(crate) fn os_err(context: &'static str, e: &windows::core::Error) -> WinError {
    WinError::Os {
        context,
        code: e.code().0 as u32,
    }
}

pub(crate) fn enumerator() -> Result<IMMDeviceEnumerator, WinError> {
    // SAFETY: COM is initialised on this thread by the caller.
    unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
        .map_err(|e| os_err("create device enumerator", &e))
}

pub(crate) fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(core::iter::once(0)).collect()
}

pub(crate) fn pcwstr(w: &[u16]) -> PCWSTR {
    PCWSTR(w.as_ptr())
}

pub(crate) fn take_pwstr(p: PWSTR) -> String {
    if p.is_null() {
        return String::new();
    }
    // SAFETY: COM-allocated, null-terminated string owned by us; freed once.
    unsafe {
        let s = p.to_string().unwrap_or_default();
        CoTaskMemFree(Some(p.0 as *const core::ffi::c_void));
        s
    }
}

/// Read a null-terminated UTF-16 PCWSTR provided by a COM callback.
pub(crate) fn pcwstr_to_string(p: &PCWSTR) -> String {
    if p.is_null() {
        return String::new();
    }
    // SAFETY: the OS passes a valid null-terminated string for the call.
    unsafe { p.to_string() }.unwrap_or_default()
}
