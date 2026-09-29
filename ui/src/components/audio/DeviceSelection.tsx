import { useConfig, useDevices, useSetConfig } from "@/lib/queries";

import { DevicePicker } from "./DevicePicker";

/** Microphone + headphones pickers, shared by the Audio tab and the wizard. */
export function DeviceSelection() {
  const config = useConfig();
  const devices = useDevices();
  const setConfig = useSetConfig();

  if (!config.data) return null;
  const list = devices.data ?? [];
  const busy = setConfig.isPending || devices.isPending;

  return (
    <div className="flex flex-col gap-4">
      <DevicePicker
        id="mic-device"
        label="Microphone"
        direction="capture"
        devices={list}
        value={config.data.captureDevice}
        disabled={busy}
        onChange={(captureDevice) => setConfig.mutate({ captureDevice })}
      />
      <DevicePicker
        id="headphones-device"
        label="Headphones"
        direction="render"
        devices={list}
        value={config.data.monitorDevice}
        disabled={busy}
        onChange={(monitorDevice) => setConfig.mutate({ monitorDevice })}
      />
      {list.some((d) => d.isVbCable) && (
        <p className="text-xs text-muted-foreground">
          VB-Cable devices are hidden here — the tuner sends its output to them automatically.
        </p>
      )}
    </div>
  );
}
