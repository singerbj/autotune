import { BluetoothIcon, CableIcon, CpuIcon, StarIcon } from "lucide-react";

import type { DeviceInfo } from "@/bindings";
import { Badge } from "@/components/ui/badge";

/** Default / ASIO / VB-Cable / Bluetooth markers for a device (FR-01). */
export function DeviceBadges({ device }: { device: DeviceInfo }) {
  return (
    <span className="flex flex-wrap items-center gap-1">
      {device.isDefault && (
        <Badge variant="secondary">
          <StarIcon aria-hidden />
          Default
        </Badge>
      )}
      {device.asioDriver !== null && (
        <Badge variant="default" title={device.asioDriver}>
          <CpuIcon aria-hidden />
          ASIO
        </Badge>
      )}
      {device.isVbCable && (
        <Badge variant="outline">
          <CableIcon aria-hidden />
          VB-Cable
        </Badge>
      )}
      {device.isBluetooth && (
        <Badge variant="warning" title="Bluetooth adds a lot of latency; wired is recommended.">
          <BluetoothIcon aria-hidden />
          Bluetooth
        </Badge>
      )}
    </span>
  );
}
