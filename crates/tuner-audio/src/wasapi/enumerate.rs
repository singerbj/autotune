//! Endpoint enumeration (FR-01).

use windows::Win32::Devices::FunctionDiscovery::{
    PKEY_Device_ContainerId, PKEY_Device_EnumeratorName, PKEY_Device_FriendlyName,
};
use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::Media::Audio::{
    eCapture, eCommunications, eConsole, eRender, EDataFlow, IMMDevice, IMMDeviceEnumerator,
    MMDeviceEnumerator, DEVICE_STATE_ACTIVE,
};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL, STGM_READ};
use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;

use super::com::{map_err, pcwstr, take_pwstr, wide};
use crate::types::{is_vb_cable_name, AudioError, DeviceInfo, Direction};

pub(crate) fn enumerator() -> Result<IMMDeviceEnumerator, AudioError> {
    // SAFETY: COM is initialised on this thread by the caller (ComGuard).
    unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
        .map_err(|e| map_err("create device enumerator", &e))
}

pub(crate) fn flow(dir: Direction) -> EDataFlow {
    match dir {
        Direction::Capture => eCapture,
        Direction::Render => eRender,
    }
}

pub(crate) fn device_id(dev: &IMMDevice) -> Result<String, AudioError> {
    // SAFETY: `dev` is a valid endpoint; GetId returns a COM string we free.
    unsafe { dev.GetId() }
        .map(take_pwstr)
        .map_err(|e| map_err("IMMDevice::GetId", &e))
}

fn prop(store: &IPropertyStore, key: &PROPERTYKEY) -> Option<String> {
    // SAFETY: `store` is a valid property store and `key` a valid key; the
    // returned PROPVARIANT owns its data and is cleared on drop.
    let v = unsafe { store.GetValue(key) }.ok()?;
    if v.is_empty() {
        return None;
    }
    let s = v.to_string();
    (!s.is_empty()).then_some(s)
}

/// Friendly name of an endpoint.
pub(crate) fn device_name(dev: &IMMDevice) -> String {
    // SAFETY: `dev` is a valid endpoint.
    unsafe { dev.OpenPropertyStore(STGM_READ) }
        .ok()
        .and_then(|s| prop(&s, &PKEY_Device_FriendlyName))
        .unwrap_or_else(|| "Unknown device".into())
}

/// Resolve an endpoint by ID, or the console default for `dir`.
pub(crate) fn get_device(
    en: &IMMDeviceEnumerator,
    id: Option<&str>,
    dir: Direction,
) -> Result<IMMDevice, AudioError> {
    match id {
        Some(id) => {
            let w = wide(id);
            // SAFETY: `w` is a null-terminated UTF-16 string alive for the call.
            unsafe { en.GetDevice(pcwstr(&w)) }.map_err(|e| match map_err("GetDevice", &e) {
                AudioError::DeviceNotFound(_) => AudioError::DeviceNotFound(id.to_string()),
                other => other,
            })
        }
        // SAFETY: plain COM call on a valid enumerator.
        None => unsafe { en.GetDefaultAudioEndpoint(flow(dir), eConsole) }
            .map_err(|_| AudioError::NoDevice),
    }
}

pub(crate) fn list_devices() -> Result<Vec<DeviceInfo>, AudioError> {
    let en = enumerator()?;
    let mut out = Vec::new();
    for dir in [Direction::Capture, Direction::Render] {
        let default_id = |role| {
            // SAFETY: plain COM call on a valid enumerator.
            unsafe { en.GetDefaultAudioEndpoint(flow(dir), role) }
                .ok()
                .and_then(|d| device_id(&d).ok())
        };
        let console = default_id(eConsole);
        let comms = default_id(eCommunications);
        // SAFETY: plain COM calls on valid interfaces.
        let coll = unsafe { en.EnumAudioEndpoints(flow(dir), DEVICE_STATE_ACTIVE) }
            .map_err(|e| map_err("EnumAudioEndpoints", &e))?;
        // SAFETY: as above.
        let count = unsafe { coll.GetCount() }.map_err(|e| map_err("GetCount", &e))?;
        for i in 0..count {
            // SAFETY: `i < count`.
            let Ok(dev) = (unsafe { coll.Item(i) }) else {
                continue;
            };
            let Ok(id) = device_id(&dev) else { continue };
            // SAFETY: valid endpoint.
            let store = unsafe { dev.OpenPropertyStore(STGM_READ) }.ok();
            let get = |k| store.as_ref().and_then(|s| prop(s, k));
            let name = get(&PKEY_Device_FriendlyName).unwrap_or_else(|| "Unknown device".into());
            let enumerator = get(&PKEY_Device_EnumeratorName).unwrap_or_default();
            out.push(DeviceInfo {
                is_default: console.as_deref() == Some(id.as_str()),
                is_default_communications: comms.as_deref() == Some(id.as_str()),
                is_vb_cable: is_vb_cable_name(&name),
                is_bluetooth: enumerator.to_ascii_uppercase().starts_with("BTH"),
                asio_driver: None,
                container_id: get(&PKEY_Device_ContainerId),
                id,
                name,
                direction: dir,
            });
        }
    }
    Ok(out)
}
