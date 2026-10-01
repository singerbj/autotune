import { screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { commands } from "@/lib/api";
import { renderWithClient } from "@/test/render";

import { TunePanel } from "./TunePanel";

async function renderPanel() {
  const utils = renderWithClient(<TunePanel />);
  await screen.findByRole("combobox", { name: "Key" });
  return utils;
}

async function choose(
  user: ReturnType<typeof renderWithClient>["user"],
  name: string,
  option: string,
) {
  await user.click(screen.getByRole("combobox", { name }));
  const listbox = await screen.findByRole("listbox");
  await user.click(within(listbox).getByRole("option", { name: option }));
}

describe("TunePanel (FR-06 – FR-11)", () => {
  it("FR-06: changing the key sends only { key }", async () => {
    const spy = vi.spyOn(commands, "setParams");
    const { user } = await renderPanel();
    await choose(user, "Key", "D");
    await waitFor(() => expect(spy).toHaveBeenCalledTimes(1));
    expect(spy).toHaveBeenCalledWith({ key: 2 });
    await waitFor(() =>
      expect(screen.getByRole("combobox", { name: "Key" })).toHaveTextContent("D"),
    );
  });

  it("FR-06: changing the scale sends only { scale }", async () => {
    const spy = vi.spyOn(commands, "setParams");
    const { user } = await renderPanel();
    await choose(user, "Scale", "Natural minor");
    await waitFor(() => expect(spy).toHaveBeenCalledWith({ scale: "naturalMinor" }));
    expect(spy).toHaveBeenCalledTimes(1);
  });

  it("FR-08: choosing a voice range sends only { voiceRange }", async () => {
    const spy = vi.spyOn(commands, "setParams");
    const { user } = await renderPanel();
    expect(screen.getByText(/adds ≈10 ms/)).toBeInTheDocument();
    await user.click(screen.getByRole("radio", { name: "Low voice range" }));
    await waitFor(() => expect(spy).toHaveBeenCalledWith({ voiceRange: "low" }));
    expect(spy).toHaveBeenCalledTimes(1);
    expect(await screen.findByText(/Tracks down to 70 Hz · adds ≈14 ms/)).toBeInTheDocument();
  });

  it("FR-06: the custom keyboard appears for the custom scale and toggles mask bits", async () => {
    await commands.setParams({ scale: "custom", customMask: 0xab5 });
    const spy = vi.spyOn(commands, "setParams");
    const { user } = await renderPanel();

    const keys = screen.getByRole("group", { name: "Custom scale notes" });
    const cSharp = within(keys).getByRole("button", { name: /^C# / });
    expect(cSharp).toHaveAttribute("aria-pressed", "false");
    await user.click(cSharp);
    await waitFor(() => expect(spy).toHaveBeenCalledWith({ customMask: 0xab5 | 0b10 }));
    await waitFor(() => expect(cSharp).toHaveAttribute("aria-pressed", "true"));

    // Turning off B (bit 11) clears only that bit.
    await user.click(within(keys).getByRole("button", { name: /^B / }));
    await waitFor(() =>
      expect(spy).toHaveBeenLastCalledWith({ customMask: (0xab5 | 0b10) & ~0x800 }),
    );
  });

  it("hides the custom keyboard for built-in scales", async () => {
    await renderPanel();
    expect(screen.queryByRole("group", { name: "Custom scale notes" })).not.toBeInTheDocument();
  });

  it("FR-21: saves and loads presets", async () => {
    const save = vi.spyOn(commands, "savePreset");
    const load = vi.spyOn(commands, "loadPreset");
    const { user } = await renderPanel();
    await user.type(screen.getByRole("textbox", { name: "Preset name" }), "Karaoke");
    await user.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(save).toHaveBeenCalledWith("Karaoke"));
    expect(await screen.findByRole("button", { name: "Load preset Karaoke" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Load preset Hard tune" }));
    await waitFor(() => expect(load).toHaveBeenCalledWith("Hard tune"));
    await waitFor(() => expect(screen.getByText("0 ms")).toBeInTheDocument());
  });

  it("FR-23: hard tune sends { hardTune } and locks retune and humanize", async () => {
    const spy = vi.spyOn(commands, "setParams");
    const { user } = await renderPanel();
    const retune = screen.getByRole("slider", { name: "Retune speed" });
    expect(retune).not.toHaveAttribute("data-disabled");
    await user.click(screen.getByRole("switch", { name: "Hard tune" }));
    await waitFor(() => expect(spy).toHaveBeenCalledWith({ hardTune: true }));
    await waitFor(() =>
      expect(screen.getByRole("slider", { name: "Retune speed" })).toHaveAttribute("data-disabled"),
    );
    expect(screen.getByText("Hard tune always snaps at 0 ms.")).toBeInTheDocument();
  });

  it("FR-23: suggests a scale when hard tune runs on chromatic", async () => {
    await commands.setParams({ scale: "chromatic", hardTune: true });
    await renderPanel();
    expect(screen.getByText(/choose your song's key/)).toBeInTheDocument();
  });

  it("FR-24: the formant slider sends { formantSemitones }", async () => {
    const spy = vi.spyOn(commands, "setParams");
    const { user } = await renderPanel();
    const formant = screen.getByRole("slider", { name: "Formant" });
    await user.click(formant);
    await user.keyboard("{ArrowLeft}");
    await waitFor(() => expect(spy).toHaveBeenCalledWith({ formantSemitones: -0.5 }));
  });

  it("FR-27: applies a built-in style and keeps the key", async () => {
    const apply = vi.spyOn(commands, "applyStyle");
    await commands.setParams({ key: 7, scale: "chromatic" });
    const { user } = await renderPanel();
    await user.click(await screen.findByRole("button", { name: "Apply style Chrome Snap" }));
    await waitFor(() => expect(apply).toHaveBeenCalledWith("chromeSnap"));
    await waitFor(() =>
      expect(screen.getByRole("switch", { name: "Hard tune" })).toHaveAttribute(
        "aria-checked",
        "true",
      ),
    );
    expect(screen.getByRole("combobox", { name: "Key" })).toHaveTextContent("G");
    expect(screen.getByRole("combobox", { name: "Scale" })).toHaveTextContent("Major");
    expect(screen.getByRole("button", { name: "Apply style Night Drive" })).toBeInTheDocument();
  });
});
