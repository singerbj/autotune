import type { DeviceInfo, Direction } from "@/bindings";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";

import { DeviceBadges } from "./DeviceBadges";

/** Radix Select can't use "" as a value, so "System default" (= null) gets a sentinel. */
const DEFAULT_VALUE = "__system_default__";

/**
 * Devices a picker should offer: the right direction only, and no VB-Cable
 * endpoints (they carry the tuned voice to other apps) unless one is already
 * selected, in which case it stays visible and clearly marked.
 */
export function pickerDevices(
  devices: readonly DeviceInfo[],
  direction: Direction,
  selectedId: string | null,
): DeviceInfo[] {
  return devices.filter((d) => d.direction === direction && (!d.isVbCable || d.id === selectedId));
}

export function DevicePicker({
  id,
  label,
  direction,
  devices,
  value,
  onChange,
  disabled,
}: {
  id: string;
  label: string;
  direction: Direction;
  devices: readonly DeviceInfo[];
  value: string | null;
  onChange: (deviceId: string | null) => void;
  disabled?: boolean;
}) {
  const options = pickerDevices(devices, direction, value);
  const systemDefault = devices.find((d) => d.direction === direction && d.isDefault);
  const selected = value === null ? systemDefault : options.find((d) => d.id === value);
  const missing = value !== null && selected === undefined;

  return (
    <div className="flex flex-col gap-2">
      <Label htmlFor={id}>{label}</Label>
      <Select
        value={value ?? DEFAULT_VALUE}
        disabled={disabled}
        onValueChange={(v) => onChange(v === DEFAULT_VALUE ? null : v)}
      >
        <SelectTrigger id={id}>
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem
            value={DEFAULT_VALUE}
            aside={
              systemDefault && (
                <span className="truncate text-xs text-muted-foreground">
                  ({systemDefault.name})
                </span>
              )
            }
          >
            System default
          </SelectItem>
          {options.map((d) => (
            <SelectItem key={d.id} value={d.id} aside={<DeviceBadges device={d} />}>
              {d.name}
            </SelectItem>
          ))}
          {missing && (
            <SelectItem value={value} disabled>
              Disconnected device
            </SelectItem>
          )}
        </SelectContent>
      </Select>
      <div className="flex min-h-5 flex-wrap items-center gap-2 text-xs text-muted-foreground">
        {missing ? (
          <span className="text-warning">The saved device is not connected.</span>
        ) : (
          selected && (
            <>
              {value === null && <span>Using {selected.name}</span>}
              <DeviceBadges device={selected} />
            </>
          )
        )}
      </div>
    </div>
  );
}
