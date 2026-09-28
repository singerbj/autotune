//! `IMMNotificationClient` → [`DeviceEvent`]s (FR-01 live list, FR-05).
//!
//! Registration happens on a dedicated thread that owns the COM objects and
//! parks until the watcher is dropped, so no COM pointer crosses threads.

use std::sync::mpsc::{channel, Sender};
use std::thread::JoinHandle;

use windows::core::{implement, PCWSTR};
use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::Media::Audio::{
    eCapture, eConsole, EDataFlow, ERole, IMMNotificationClient, IMMNotificationClient_Impl,
    DEVICE_STATE,
};

use super::{enumerator, pcwstr_to_string, Com};
use crate::{DeviceEvent, WinError};

type Callback = Box<dyn Fn(DeviceEvent) + Send + Sync>;

#[implement(IMMNotificationClient)]
struct Client {
    cb: Callback,
}

impl IMMNotificationClient_Impl for Client_Impl {
    fn OnDeviceStateChanged(&self, id: &PCWSTR, _state: DEVICE_STATE) -> windows::core::Result<()> {
        (self.cb)(DeviceEvent::StateChanged(pcwstr_to_string(id)));
        Ok(())
    }

    fn OnDeviceAdded(&self, id: &PCWSTR) -> windows::core::Result<()> {
        (self.cb)(DeviceEvent::Added(pcwstr_to_string(id)));
        Ok(())
    }

    fn OnDeviceRemoved(&self, id: &PCWSTR) -> windows::core::Result<()> {
        (self.cb)(DeviceEvent::Removed(pcwstr_to_string(id)));
        Ok(())
    }

    fn OnDefaultDeviceChanged(
        &self,
        flow: EDataFlow,
        role: ERole,
        id: &PCWSTR,
    ) -> windows::core::Result<()> {
        if role == eConsole {
            (self.cb)(DeviceEvent::DefaultChanged {
                capture: flow == eCapture,
                id: pcwstr_to_string(id),
            });
        }
        Ok(())
    }

    fn OnPropertyValueChanged(
        &self,
        _id: &PCWSTR,
        _key: &PROPERTYKEY,
    ) -> windows::core::Result<()> {
        Ok(())
    }
}

pub(crate) struct Watcher {
    stop: Option<Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl Watcher {
    pub(crate) fn start(cb: Callback) -> Result<Self, WinError> {
        let (stop_tx, stop_rx) = channel::<()>();
        let (ready_tx, ready_rx) = channel::<Result<(), WinError>>();
        let thread = std::thread::Builder::new()
            .name("device-watcher".into())
            .spawn(move || {
                let _com = Com::init();
                let en = match enumerator() {
                    Ok(e) => e,
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                let client: IMMNotificationClient = Client { cb }.into();
                // SAFETY: valid enumerator and client; unregistered below.
                if let Err(e) = unsafe { en.RegisterEndpointNotificationCallback(&client) } {
                    let _ = ready_tx.send(Err(super::os_err(
                        "RegisterEndpointNotificationCallback",
                        &e,
                    )));
                    return;
                }
                let _ = ready_tx.send(Ok(()));
                let _ = stop_rx.recv();
                // SAFETY: same enumerator and client that were registered.
                let _ = unsafe { en.UnregisterEndpointNotificationCallback(&client) };
            })
            .map_err(|_| WinError::Unsupported("could not start the device watcher thread"))?;
        ready_rx
            .recv()
            .map_err(|_| WinError::Unsupported("device watcher thread exited"))??;
        Ok(Self {
            stop: Some(stop_tx),
            thread: Some(thread),
        })
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        drop(self.stop.take());
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}
