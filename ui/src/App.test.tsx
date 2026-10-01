import { screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { commands } from "@/lib/api";
import { renderWithClient } from "@/test/render";

import { App } from "./App";

describe("App shell", () => {
  it("FR-02/FR-16: shows engine state, tier and latency in the header", async () => {
    renderWithClient(<App />);
    expect(await screen.findByText("WASAPI exclusive")).toBeInTheDocument();
    expect(screen.getByText("Running")).toBeInTheDocument();
    expect(screen.getByText(/~\d+ ms/)).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Tune" })).toHaveAttribute("aria-selected", "true");
  });

  it("stops and starts the engine", async () => {
    const stop = vi.spyOn(commands, "stopEngine");
    const start = vi.spyOn(commands, "startEngine");
    const { user } = renderWithClient(<App />);
    await user.click(await screen.findByRole("button", { name: "Stop" }));
    expect(stop).toHaveBeenCalled();
    expect(await screen.findByText("Stopped")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Start" }));
    expect(start).toHaveBeenCalled();
    expect(await screen.findByText("Running")).toBeInTheDocument();
  });

  it("FR-25: has an Effects tab", async () => {
    const { user } = renderWithClient(<App />);
    await user.click(await screen.findByRole("tab", { name: "Effects" }));
    expect(await screen.findByRole("slider", { name: "Reverb level" })).toBeInTheDocument();
  });

  it("switches tabs and shows diagnostics", async () => {
    const { user } = renderWithClient(<App />);
    await user.click(await screen.findByRole("tab", { name: "Diagnostics" }));
    expect(await screen.findByRole("button", { name: "Copy diagnostics" })).toBeInTheDocument();
    expect(screen.getByText("Headset Microphone (Arctis 7)")).toBeInTheDocument();
  });

  it("FR-10/FR-20: records a new tuning hotkey and applies it", async () => {
    const setConfig = vi.spyOn(commands, "setConfig");
    const { user } = renderWithClient(<App />);
    await user.click(await screen.findByRole("tab", { name: "Settings" }));
    await user.click(await screen.findByRole("button", { name: /Change/ }));
    expect(screen.getByText(/Press a key combination/)).toBeInTheDocument();
    await user.keyboard("{Control>}{Shift>}t{/Shift}{/Control}");
    expect(setConfig).toHaveBeenCalledWith({ bypassHotkey: "Ctrl+Shift+T" });
    expect(await screen.findByText("T", { selector: "kbd" })).toBeInTheDocument();
    // The header toggle names the new hotkey.
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Tuning" })).toHaveAttribute(
        "title",
        expect.stringContaining("Ctrl+Shift+T"),
      ),
    );
    await user.click(screen.getByRole("button", { name: "Reset" }));
    expect(setConfig).toHaveBeenLastCalledWith({ bypassHotkey: "CommandOrControl+Alt+B" });
  });

  it("FR-20: shows the AppError message for an invalid hotkey and keeps the old one", async () => {
    const { user } = renderWithClient(<App />);
    await user.click(await screen.findByRole("tab", { name: "Settings" }));
    await user.click(await screen.findByRole("button", { name: /Change/ }));
    await user.keyboard("b");
    await waitFor(() => expect(screen.getByRole("alert")).toHaveTextContent("needs a modifier"));
    expect(screen.getByText("B", { selector: "kbd" })).toBeInTheDocument();
    expect(screen.getByText("Alt", { selector: "kbd" })).toBeInTheDocument();
  });

  it("FR-20: Esc cancels recording a hotkey", async () => {
    const setConfig = vi.spyOn(commands, "setConfig");
    const { user } = renderWithClient(<App />);
    await user.click(await screen.findByRole("tab", { name: "Settings" }));
    await user.click(await screen.findByRole("button", { name: /Change/ }));
    await user.keyboard("{Escape}");
    expect(screen.queryByText(/Press a key combination/)).not.toBeInTheDocument();
    expect(setConfig).not.toHaveBeenCalled();
  });

  it("FR-22: shows the update-ready button after a check", async () => {
    const { user } = renderWithClient(<App />);
    await user.click(await screen.findByRole("tab", { name: "Settings" }));
    await user.click(await screen.findByRole("button", { name: "Check for updates" }));
    expect(
      await screen.findByRole("button", { name: /Update ready — restart/ }),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Restart and install" })).toBeInTheDocument();
  });
});
