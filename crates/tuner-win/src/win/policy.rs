//! Undocumented `IPolicyConfig` (Windows 7+), used to set default endpoints
//! and to re-enable a disabled VB-Cable endpoint (what Sound settings does).
//! Isolated here behind [`DefaultEndpointControl`]; any failure surfaces as
//! an error and the UI falls back to manual steps.

#![allow(non_snake_case)]

use core::ffi::c_void;

use windows::core::{interface, IUnknown, IUnknown_Vtbl, GUID, HRESULT, PCWSTR};
use windows::Win32::Media::Audio::{
    eCapture, eCommunications, eConsole, eMultimedia, eRender, EDataFlow, ERole,
};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};

use super::{enumerator, os_err, pcwstr, take_pwstr, wide, Com};
use crate::routing::{DefaultEndpointControl, Flow, Role};
use crate::WinError;

const CLSID_POLICY_CONFIG_CLIENT: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);

/// Vtable order matters; only `SetDefaultEndpoint` and
/// `SetEndpointVisibility` are called. Method names
/// mirror the COM interface.
#[interface("f8679f50-850a-41cf-9c72-430f290290c8")]
unsafe trait IPolicyConfig: IUnknown {
    fn GetMixFormat(&self, device: PCWSTR, format: *mut *mut c_void) -> HRESULT;
    fn GetDeviceFormat(&self, device: PCWSTR, default: i32, format: *mut *mut c_void) -> HRESULT;
    fn ResetDeviceFormat(&self, device: PCWSTR) -> HRESULT;
    fn SetDeviceFormat(&self, device: PCWSTR, endpoint: *mut c_void, mix: *mut c_void) -> HRESULT;
    fn GetProcessingPeriod(
        &self,
        device: PCWSTR,
        default: i32,
        def: *mut i64,
        min: *mut i64,
    ) -> HRESULT;
    fn SetProcessingPeriod(&self, device: PCWSTR, period: *mut i64) -> HRESULT;
    fn GetShareMode(&self, device: PCWSTR, mode: *mut c_void) -> HRESULT;
    fn SetShareMode(&self, device: PCWSTR, mode: *mut c_void) -> HRESULT;
    fn GetPropertyValue(
        &self,
        device: PCWSTR,
        fx: i32,
        key: *const c_void,
        value: *mut c_void,
    ) -> HRESULT;
    fn SetPropertyValue(
        &self,
        device: PCWSTR,
        fx: i32,
        key: *const c_void,
        value: *mut c_void,
    ) -> HRESULT;
    fn SetDefaultEndpoint(&self, device: PCWSTR, role: ERole) -> HRESULT;
    fn SetEndpointVisibility(&self, device: PCWSTR, visible: i32) -> HRESULT;
}

fn erole(r: Role) -> ERole {
    match r {
        Role::Console => eConsole,
        Role::Multimedia => eMultimedia,
        Role::Communications => eCommunications,
    }
}

fn eflow(f: Flow) -> EDataFlow {
    match f {
        Flow::Render => eRender,
        Flow::Capture => eCapture,
    }
}

fn policy_config() -> Result<IPolicyConfig, WinError> {
    // SAFETY: COM initialised by the caller; the class is registered on Windows 7+.
    unsafe { CoCreateInstance(&CLSID_POLICY_CONFIG_CLIENT, None, CLSCTX_ALL) }
        .map_err(|_| WinError::Unsupported("IPolicyConfig is not available on this Windows build"))
}

/// Real implementation.
#[derive(Debug, Default, Clone, Copy)]
pub struct PolicyConfigControl;

impl DefaultEndpointControl for PolicyConfigControl {
    fn get_default_capture(&self, role: Role) -> Result<Option<String>, WinError> {
        self.get_default(Flow::Capture, role)
    }

    fn set_default_capture(&self, id: &str, role: Role) -> Result<(), WinError> {
        let _com = Com::init();
        let policy = policy_config()?;
        let w = wide(id);
        // SAFETY: `w` is a null-terminated endpoint ID alive for the call.
        unsafe { policy.SetDefaultEndpoint(pcwstr(&w), erole(role)) }
            .ok()
            .map_err(|e| os_err("IPolicyConfig::SetDefaultEndpoint", &e))
    }

    fn get_default(&self, flow: Flow, role: Role) -> Result<Option<String>, WinError> {
        let _com = Com::init();
        let en = enumerator()?;
        // SAFETY: valid enumerator; returns an owned endpoint.
        match unsafe { en.GetDefaultAudioEndpoint(eflow(flow), erole(role)) } {
            // SAFETY: valid endpoint; GetId returns a COM string we free.
            Ok(dev) => Ok(Some(take_pwstr(
                unsafe { dev.GetId() }.map_err(|e| os_err("IMMDevice::GetId", &e))?,
            ))),
            Err(_) => Ok(None),
        }
    }

    fn set_endpoint_enabled(&self, id: &str, enabled: bool) -> Result<(), WinError> {
        let _com = Com::init();
        let policy = policy_config()?;
        let w = wide(id);
        // SAFETY: `w` is a null-terminated endpoint ID alive for the call.
        unsafe { policy.SetEndpointVisibility(pcwstr(&w), i32::from(enabled)) }
            .ok()
            .map_err(|e| os_err("IPolicyConfig::SetEndpointVisibility", &e))
    }
}
