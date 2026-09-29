import { act, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { commands, events } from "@/lib/api";
import { useBackendEvents } from "@/lib/useBackendEvents";
import { renderWithClient } from "@/test/render";

import { BypassToggle } from "./BypassToggle";

function WithEvents() {
  useBackendEvents();
  return <BypassToggle />;
}

describe("BypassToggle (FR-10)", () => {
  it("calls setBypass with the toggled value", async () => {
    const spy = vi.spyOn(commands, "setBypass");
    const { user } = renderWithClient(<BypassToggle />);
    const button = await screen.findByRole("button", { name: "Bypass" });
    await waitFor(() => expect(button).toBeEnabled());
    expect(button).toHaveAttribute("aria-pressed", "false");

    await user.click(button);
    expect(spy).toHaveBeenCalledWith(true);
    await waitFor(() => expect(button).toHaveAttribute("aria-pressed", "true"));

    await user.click(button);
    expect(spy).toHaveBeenLastCalledWith(false);
    await waitFor(() => expect(button).toHaveAttribute("aria-pressed", "false"));
  });

  it("follows bypassEvent from the tray / hotkey", async () => {
    renderWithClient(<WithEvents />);
    const button = await screen.findByRole("button", { name: "Bypass" });
    await waitFor(() => expect(button).toBeEnabled());
    await act(async () => {
      await events.bypassEvent.emit(true);
    });
    await waitFor(() => expect(button).toHaveAttribute("aria-pressed", "true"));
  });
});
