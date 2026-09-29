import { screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import type { DeviceInfo } from "@/bindings";
import { renderWithClient } from "@/test/render";

import { DevicePicker, pickerDevices } from "./DevicePicker";

function device(
  partial: Partial<DeviceInfo> & Pick<DeviceInfo, "id" | "name" | "direction">,
): DeviceInfo {
  return {
    isDefault: false,
    isDefaultCommunications: false,
    isVbCable: false,
    isBluetooth: false,
    asioDriver: null,
    containerId: null,
    ...partial,
  };
}

const DEVICES: DeviceInfo[] = [
  device({ id: "mic", name: "Headset Mic", direction: "capture", isDefault: true }),
  device({ id: "asio", name: "Focusrite In", direction: "capture", asioDriver: "Focusrite ASIO" }),
  device({ id: "bt", name: "AirPods Mic", direction: "capture", isBluetooth: true }),
  device({ id: "cable-out", name: "CABLE Output", direction: "capture", isVbCable: true }),
  device({ id: "phones", name: "Headphones", direction: "render", isDefault: true }),
  device({ id: "cable-in", name: "CABLE Input", direction: "render", isVbCable: true }),
];

describe("DevicePicker (FR-01)", () => {
  it("filters by direction and hides VB-Cable unless selected", () => {
    expect(pickerDevices(DEVICES, "capture", null).map((d) => d.id)).toEqual(["mic", "asio", "bt"]);
    expect(pickerDevices(DEVICES, "render", null).map((d) => d.id)).toEqual(["phones"]);
    expect(pickerDevices(DEVICES, "capture", "cable-out").map((d) => d.id)).toContain("cable-out");
  });

  it("lists only capture devices with badges, plus System default", async () => {
    const onChange = vi.fn<(id: string | null) => void>();
    const { user } = renderWithClient(
      <DevicePicker
        id="mic"
        label="Microphone"
        direction="capture"
        devices={DEVICES}
        value={null}
        onChange={onChange}
      />,
    );
    await user.click(screen.getByRole("combobox", { name: "Microphone" }));
    const listbox = await screen.findByRole("listbox");
    const options = within(listbox).getAllByRole("option");
    expect(options.map((o) => o.textContent)).toEqual([
      expect.stringContaining("System default"),
      expect.stringContaining("Headset Mic"),
      expect.stringContaining("Focusrite In"),
      expect.stringContaining("AirPods Mic"),
    ]);
    expect(within(listbox).queryByText("Headphones")).not.toBeInTheDocument();
    expect(within(listbox).queryByText("CABLE Output")).not.toBeInTheDocument();

    const [, headset, focusrite, airpods] = options;
    expect(headset).toHaveTextContent("Default");
    expect(focusrite).toHaveTextContent("ASIO");
    expect(airpods).toHaveTextContent("Bluetooth");

    await user.click(within(listbox).getByRole("option", { name: /Focusrite In/ }));
    expect(onChange).toHaveBeenCalledWith("asio");
  });

  it("maps System default to null", async () => {
    const onChange = vi.fn<(id: string | null) => void>();
    const { user } = renderWithClient(
      <DevicePicker
        id="phones"
        label="Headphones"
        direction="render"
        devices={DEVICES}
        value="phones"
        onChange={onChange}
      />,
    );
    await user.click(screen.getByRole("combobox", { name: "Headphones" }));
    const listbox = await screen.findByRole("listbox");
    expect(within(listbox).getAllByRole("option")).toHaveLength(2);
    await user.click(within(listbox).getByRole("option", { name: /System default/ }));
    expect(onChange).toHaveBeenCalledWith(null);
  });

  it("keeps a selected VB-Cable device visible and marked", () => {
    renderWithClient(
      <DevicePicker
        id="mic"
        label="Microphone"
        direction="capture"
        devices={DEVICES}
        value="cable-out"
        onChange={() => {}}
      />,
    );
    expect(screen.getByRole("combobox", { name: "Microphone" })).toHaveTextContent("CABLE Output");
    expect(screen.getByText("VB-Cable")).toBeInTheDocument();
  });
});
