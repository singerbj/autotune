import { describe, expect, it } from "vitest";

import { acceleratorFor, hotkeyKeys, hotkeyLabel } from "./hotkey";

const press = (
  code: string,
  mods: Partial<Record<"ctrl" | "alt" | "shift" | "meta", boolean>> = {},
) =>
  acceleratorFor({
    code,
    ctrlKey: mods.ctrl ?? false,
    altKey: mods.alt ?? false,
    shiftKey: mods.shift ?? false,
    metaKey: mods.meta ?? false,
  });

describe("hotkey helpers (FR-10)", () => {
  it("turns key presses into accelerators", () => {
    expect(press("KeyB", { ctrl: true, alt: true })).toBe("Ctrl+Alt+B");
    expect(press("Digit7", { shift: true, meta: true })).toBe("Shift+Super+7");
    expect(press("F9")).toBe("F9");
    expect(press("Numpad1", { ctrl: true })).toBe("Ctrl+Numpad1");
    expect(press("ArrowUp", { alt: true })).toBe("Alt+ArrowUp");
    expect(press("Backquote", { ctrl: true })).toBe("Ctrl+Backquote");
  });

  it("waits while only modifiers are held", () => {
    expect(press("ControlLeft", { ctrl: true })).toBeNull();
    expect(press("AltRight", { alt: true })).toBeNull();
    expect(press("MetaLeft", { meta: true })).toBeNull();
    expect(press("")).toBeNull();
  });

  it("shows accelerators as key caps", () => {
    expect(hotkeyKeys("CommandOrControl+Alt+b")).toEqual(["Ctrl", "Alt", "B"]);
    expect(hotkeyKeys("Super+KeyQ")).toEqual(["Win", "Q"]);
    expect(hotkeyKeys("Ctrl+Numpad3")).toEqual(["Ctrl", "Num3"]);
    expect(hotkeyLabel("Alt+ArrowUp")).toBe("Alt+Up");
    expect(hotkeyKeys("")).toEqual([]);
  });
});
