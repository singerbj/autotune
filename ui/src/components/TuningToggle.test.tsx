import { act, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { commands, events } from "@/lib/api";
import { useBackendEvents } from "@/lib/useBackendEvents";
import { renderWithClient } from "@/test/render";

import { TuningToggle } from "./TuningToggle";

function WithEvents() {
  useBackendEvents();
  return <TuningToggle />;
}

describe("TuningToggle (FR-10)", () => {
  it("starts off and toggles bypass", async () => {
    const spy = vi.spyOn(commands, "setBypass");
    const { user } = renderWithClient(<TuningToggle />);
    const button = await screen.findByRole("button", { name: "Tuning" });
    await waitFor(() => expect(button).toBeEnabled());
    expect(button).toHaveAttribute("aria-pressed", "false");

    await user.click(button);
    expect(spy).toHaveBeenCalledWith(false);
    await waitFor(() => expect(button).toHaveAttribute("aria-pressed", "true"));

    await user.click(button);
    expect(spy).toHaveBeenLastCalledWith(true);
    await waitFor(() => expect(button).toHaveAttribute("aria-pressed", "false"));
  });

  it("names the hotkey", async () => {
    renderWithClient(<TuningToggle />);
    const button = await screen.findByRole("button", { name: "Tuning" });
    await waitFor(() =>
      expect(button).toHaveAttribute("title", expect.stringContaining("Ctrl+Alt+B")),
    );
  });

  it("follows bypassEvent from the tray / hotkey", async () => {
    renderWithClient(<WithEvents />);
    const button = await screen.findByRole("button", { name: "Tuning" });
    await waitFor(() => expect(button).toBeEnabled());
    await act(async () => {
      await events.bypassEvent.emit(false);
    });
    await waitFor(() => expect(button).toHaveAttribute("aria-pressed", "true"));
    await act(async () => {
      await events.bypassEvent.emit(true);
    });
    await waitFor(() => expect(button).toHaveAttribute("aria-pressed", "false"));
  });
});
