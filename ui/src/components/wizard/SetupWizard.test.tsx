import { screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { commands } from "@/lib/api";
import { renderWithClient } from "@/test/render";

import { SetupWizard, WIZARD_STEPS } from "./SetupWizard";

function heading() {
  return screen.getByRole("heading", { level: 2 });
}

describe("SetupWizard (FR-12, FR-13)", () => {
  it("goes forward and back, then completes the wizard", async () => {
    const complete = vi.spyOn(commands, "completeWizard");
    const onOpenChange = vi.fn<(open: boolean) => void>();
    const { user } = renderWithClient(<SetupWizard open onOpenChange={onOpenChange} />);

    expect(heading()).toHaveTextContent("Welcome to TunedUp");
    expect(screen.getByRole("button", { name: "Back" })).toBeDisabled();

    await user.click(screen.getByRole("button", { name: "Next" }));
    expect(heading()).toHaveTextContent("Choose your devices");
    expect(await screen.findByRole("combobox", { name: "Microphone" })).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Back" }));
    expect(heading()).toHaveTextContent("Welcome to TunedUp");

    await user.click(screen.getByRole("button", { name: "Next" }));
    await user.click(screen.getByRole("button", { name: "Next" }));
    expect(heading()).toHaveTextContent("Virtual microphone");
    expect(await screen.findByText(/VB-Cable is installed/)).toBeInTheDocument();
    expect(screen.getByText("obs64.exe")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Next" }));
    expect(heading()).toHaveTextContent("Headset checks");

    await user.click(screen.getByRole("button", { name: "Next" }));
    expect(heading()).toHaveTextContent("Set up Discord");
    expect(screen.getByText(/Noise Suppression/)).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Next" }));
    expect(heading()).toHaveTextContent("Measure latency");

    await user.click(screen.getByRole("button", { name: "Next" }));
    expect(heading()).toHaveTextContent("Done");
    expect(
      screen.getByText(`Step ${WIZARD_STEPS.length} of ${WIZARD_STEPS.length}`),
    ).toBeInTheDocument();
    expect(complete).not.toHaveBeenCalled();

    await user.click(screen.getByRole("button", { name: "Finish" }));
    await waitFor(() => expect(complete).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(onOpenChange).toHaveBeenCalledWith(false));
  });

  it("FR-13: shows Bluetooth warnings from the setup check", async () => {
    await commands.setConfig({ captureDevice: "{0.0.1.00000000}.{c3-airpods}" });
    const { user } = renderWithClient(<SetupWizard open onOpenChange={() => {}} />);
    for (let i = 0; i < 3; i++) await user.click(screen.getByRole("button", { name: "Next" }));
    expect(await screen.findByText("Bluetooth device selected.")).toBeInTheDocument();
  });

  it("FR-16: runs the latency test on the latency step", async () => {
    const run = vi.spyOn(commands, "runLatencyTest");
    const { user } = renderWithClient(<SetupWizard open onOpenChange={() => {}} />);
    for (let i = 0; i < 5; i++) await user.click(screen.getByRole("button", { name: "Next" }));
    await user.click(await screen.findByRole("button", { name: "Measure latency" }));
    await waitFor(() => expect(run).toHaveBeenCalledTimes(1));
    expect(await screen.findByText("Hardware round trip")).toBeInTheDocument();
  });
});
