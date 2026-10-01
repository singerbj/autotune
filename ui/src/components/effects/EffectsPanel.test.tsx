import { screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { commands } from "@/lib/api";
import { renderWithClient } from "@/test/render";

import { EffectsPanel } from "./EffectsPanel";

async function renderPanel() {
  const utils = renderWithClient(<EffectsPanel />);
  await screen.findByRole("slider", { name: "Reverb level" });
  return utils;
}

describe("EffectsPanel (FR-25, FR-26)", () => {
  it("FR-25: a level slider sends only that effect field", async () => {
    const spy = vi.spyOn(commands, "setParams");
    const { user } = await renderPanel();
    expect(screen.getAllByText("Off").length).toBeGreaterThanOrEqual(3);
    await user.click(screen.getByRole("slider", { name: "Reverb level" }));
    await user.keyboard("{ArrowRight}");
    await waitFor(() => expect(spy).toHaveBeenCalledWith({ fx: { reverbMix: 0.01 } }));
    expect(await screen.findByText("1%")).toBeInTheDocument();
  });

  it("FR-26: routes the reverb to the headphones only", async () => {
    const spy = vi.spyOn(commands, "setParams");
    const { user } = await renderPanel();
    const both = screen.getByRole("radio", { name: "Reverb in both" });
    expect(both).toHaveAttribute("aria-checked", "true");
    await user.click(screen.getByRole("radio", { name: "Reverb in headphones only" }));
    await waitFor(() => expect(spy).toHaveBeenCalledWith({ fx: { reverbRoute: "headphones" } }));
    expect(spy).toHaveBeenCalledTimes(1);
    await waitFor(() =>
      expect(screen.getByRole("radio", { name: "Reverb in headphones only" })).toHaveAttribute(
        "aria-checked",
        "true",
      ),
    );
    // Other effects keep their own route.
    expect(screen.getByRole("radio", { name: "Echo in both" })).toHaveAttribute(
      "aria-checked",
      "true",
    );
  });

  it("FR-25: picks the echo note value", async () => {
    const spy = vi.spyOn(commands, "setParams");
    const { user } = await renderPanel();
    await user.click(screen.getByRole("combobox", { name: "Note" }));
    const listbox = await screen.findByRole("listbox");
    await user.click(within(listbox).getByRole("option", { name: "Dotted 1/8" }));
    await waitFor(() =>
      expect(spy).toHaveBeenCalledWith({ fx: { delayDivision: "dottedEighth" } }),
    );
  });

  it("FR-25: the compressor ratio is disabled while the compressor is off", async () => {
    await renderPanel();
    expect(screen.getByRole("slider", { name: "Compressor ratio" })).toHaveAttribute(
      "data-disabled",
    );
  });
});
