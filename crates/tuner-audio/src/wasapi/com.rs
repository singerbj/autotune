//! COM helpers.

use windows::core::{HRESULT, PCWSTR, PWSTR};
use windows::Win32::Media::Audio::{
    AUDCLNT_E_DEVICE_INVALIDATED, AUDCLNT_E_DEVICE_IN_USE, AUDCLNT_E_EXCLUSIVE_MODE_NOT_ALLOWED,
    AUDCLNT_E_UNSUPPORTED_FORMAT,
};
use windows::Win32::System::Com::{
    CoInitializeEx, CoTaskMemFree, CoUninitialize, COINIT_MULTITHREADED,
};

use crate::types::AudioError;

/// Initializes COM (MTA) for the current thread and uninitializes on drop.
/// If the thread is already STA (e.g. the UI thread) COM stays usable and
/// nothing is torn down.
#[derive(Debug)]
pub struct ComGuard {
    owned: bool,
    /// COM initialisation is per thread, so the guard must not move.
    _not_send: core::marker::PhantomData<*const ()>,
}

impl ComGuard {
    pub fn init() -> Self {
        // SAFETY: CoInitializeEx has no preconditions; a failure such as
        // RPC_E_CHANGED_MODE leaves COM usable in the existing apartment.
        let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        Self {
            owned: hr.is_ok(),
            _not_send: core::marker::PhantomData,
        }
    }
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.owned {
            // SAFETY: balanced with the successful CoInitializeEx in `init`
            // on this same thread (ComGuard is !Send).
            unsafe { CoUninitialize() };
        }
    }
}

/// Null-terminated UTF-16 copy of `s`.
pub(crate) fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(core::iter::once(0)).collect()
}

/// Use a `wide` buffer as a PCWSTR. The buffer must outlive the call.
pub(crate) fn pcwstr(w: &[u16]) -> PCWSTR {
    PCWSTR(w.as_ptr())
}

/// Convert a COM-allocated string and free it.
pub(crate) fn take_pwstr(p: PWSTR) -> String {
    if p.is_null() {
        return String::new();
    }
    // SAFETY: `p` is a valid, null-terminated string allocated by COM that we
    // own; it is read once and then freed exactly once.
    unsafe {
        let s = p.to_string().unwrap_or_default();
        CoTaskMemFree(Some(p.0 as *const core::ffi::c_void));
        s
    }
}

/// Map an audio HRESULT to a typed error.
pub(crate) fn map_err(context: &'static str, e: &windows::core::Error) -> AudioError {
    let code = e.code();
    if code == AUDCLNT_E_DEVICE_IN_USE {
        AudioError::DeviceInUse
    } else if code == AUDCLNT_E_EXCLUSIVE_MODE_NOT_ALLOWED {
        AudioError::ExclusiveNotAllowed
    } else if code == AUDCLNT_E_UNSUPPORTED_FORMAT {
        AudioError::FormatNotSupported
    } else if code == AUDCLNT_E_DEVICE_INVALIDATED {
        AudioError::DeviceInvalidated
    } else if code == HRESULT(0x8007_0490_u32 as i32) {
        // E_NOTFOUND / ERROR_NOT_FOUND from IMMDeviceEnumerator::GetDevice
        AudioError::DeviceNotFound(context.to_string())
    } else {
        AudioError::Os {
            context,
            code: code.0 as u32,
            message: e.message(),
        }
    }
}
