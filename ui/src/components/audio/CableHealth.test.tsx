import { screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import type { SetupReport } from "@/bindings";
import { CableStep } from "@/components/wizard/steps";
import { commands, mockBackend } from "@/lib/api";
import { renderWithClient } from "@/test/render";

import { CableHealth } from "./CableHealth";

const healthy: SetupReport = {
  vbCableInstalled: true,
  cableInputId: "cable-in",
  cableOutputId: "cable-out",
  inactiveCables: [],
  cableConflicts: [],
  discordDetected: false,
  discordActive: false,
  bluetoothWarning: false,
  sidetoneWarning: false,
  remoteSession: false,
  playbackOnCable: false,
  cableRepairAvailable: true,
};

const missing: SetupReport = {
  ...healthy,
  vbCableInstalled: false,
  cableInputId: null,
  cableOutputId: null,
};

function renderHealth(report: SetupReport) {
  return renderWithClient(<CableHealth report={report} rechecking={false} onRecheck={() => {}} />);
}

describe("CableHealth (FR-12, ADR 0011)", () => {
  it("shows nothing when the cable is fine", () => {
    const { container } = renderHealth(healthy);
    expect(container).toBeEmptyDOMElement();
  });

  it("installs a missing cable from the wizard", async () => {
    const devices = await commands.listDevices();
    if (devices.status !== "ok") throw new Error("mock devices");
    mockBackend?.setDevices(devices.data.filter((d) => !d.isVbCable));
    const repair = vi.spyOn(commands, "repairVirtualMic");

    const { user } = renderWithClient(<CableStep />);
    expect(await screen.findByText("VB-Cable was not found.")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Install VB-Cable" }));

    await waitFor(() => expect(repair).toHaveBeenCalledTimes(1));
    expect(await screen.findByText(/VB-Cable is installed/)).toBeInTheDocument();
    expect(screen.queryByText("VB-Cable was not found.")).not.toBeInTheDocument();
  });

  it("offers to turn a disabled cable back on", () => {
    renderHealth({
      ...missing,
      inactiveCables: [{ id: "cable-out", name: "CABLE Output", isCapture: true, disabled: true }],
    });
    expect(screen.getByRole("button", { name: "Turn VB-Cable back on" })).toBeInTheDocument();
    expect(screen.queryByText(/mmsys.cpl/)).not.toBeInTheDocument();
  });

  it("falls back to manual steps when it can't repair", () => {
    renderHealth({ ...missing, cableRepairAvailable: false });
    expect(screen.queryByRole("button", { name: "Install VB-Cable" })).not.toBeInTheDocument();
    expect(screen.getByText(/Re-run the installer/)).toBeInTheDocument();
  });

  it("says when Windows needs a restart", async () => {
    vi.spyOn(commands, "repairVirtualMic").mockResolvedValue({
      status: "ok",
      data: "restartRequired",
    });
    const { user } = renderHealth(missing);
    await user.click(screen.getByRole("button", { name: "Install VB-Cable" }));
    expect(
      await screen.findByText("Restart Windows to finish setting up VB-Cable."),
    ).toBeInTheDocument();
  });

  it("moves the default speakers off CABLE Input", async () => {
    mockBackend?.setPlaybackOnCable(true);
    const fix = vi.spyOn(commands, "fixPlaybackDevice");
    const { user } = renderWithClient(<CableStep />);
    expect(
      await screen.findByText("Windows is playing sound into CABLE Input."),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Switch back to my speakers" }));
    await waitFor(() => expect(fix).toHaveBeenCalledTimes(1));
    await waitFor(() =>
      expect(
        screen.queryByText("Windows is playing sound into CABLE Input."),
      ).not.toBeInTheDocument(),
    );
  });
});
