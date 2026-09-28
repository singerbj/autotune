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

  it("switches tabs and shows diagnostics", async () => {
    const { user } = renderWithClient(<App />);
    await user.click(await screen.findByRole("tab", { name: "Diagnostics" }));
    expect(await screen.findByRole("button", { name: "Copy diagnostics" })).toBeInTheDocument();
    expect(screen.getByText("Headset Microphone (Arctis 7)")).toBeInTheDocument();
  });

  it("FR-20: shows the AppError message for an invalid hotkey", async () => {
    const { user } = renderWithClient(<App />);
    await user.click(await screen.findByRole("tab", { name: "Settings" }));
    const field = await screen.findByRole("textbox", { name: "Bypass hotkey" });
    await user.clear(field);
    await user.type(field, "Hyper+B");
    await user.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent('Unknown modifier "Hyper"'),
    );
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
