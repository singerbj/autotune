// Global hotkey helpers (FR-10). Accelerators use the syntax the Rust side
// registers with tauri-plugin-global-shortcut: modifiers, then one key, e.g.
// "Ctrl+Alt+B" or "CommandOrControl+Shift+F9".

const MODIFIER_CODE = /^(Shift|Control|Alt|Meta|OS)(Left|Right)?$/;

/**
 * Turns a key press into an accelerator ("Ctrl+Alt+B"), or null while only
 * modifiers are held. Keys come from `KeyboardEvent.code`, so the layout
 * doesn't matter; the plugin accepts those names directly.
 */
export function acceleratorFor(
  e: Pick<KeyboardEvent, "code" | "ctrlKey" | "altKey" | "shiftKey" | "metaKey">,
): string | null {
  const { code } = e;
  if (code === "" || MODIFIER_CODE.test(code)) return null;
  let key = code;
  if (/^Key[A-Z]$/.test(code)) key = code.slice(3);
  else if (/^Digit\d$/.test(code)) key = code.slice(5);
  const mods: string[] = [];
  if (e.ctrlKey) mods.push("Ctrl");
  if (e.altKey) mods.push("Alt");
  if (e.shiftKey) mods.push("Shift");
  if (e.metaKey) mods.push("Super");
  return [...mods, key].join("+");
}

const KEY_NAMES: Record<string, string> = {
  commandorcontrol: "Ctrl",
  commandorctrl: "Ctrl",
  cmdorctrl: "Ctrl",
  cmdorcontrol: "Ctrl",
  control: "Ctrl",
  ctrl: "Ctrl",
  alt: "Alt",
  option: "Alt",
  shift: "Shift",
  super: "Win",
  meta: "Win",
  command: "Win",
  cmd: "Win",
  arrowup: "Up",
  arrowdown: "Down",
  arrowleft: "Left",
  arrowright: "Right",
  escape: "Esc",
};

/** Accelerator → key caps: "CommandOrControl+Alt+b" → ["Ctrl", "Alt", "B"]. */
export function hotkeyKeys(accelerator: string | null | undefined): string[] {
  return (accelerator ?? "")
    .split("+")
    .map((k) => k.trim())
    .filter(Boolean)
    .map((k) => {
      const lower = k.toLowerCase();
      if (KEY_NAMES[lower]) return KEY_NAMES[lower];
      if (/^key[a-z]$/.test(lower)) return lower.slice(3).toUpperCase();
      if (/^digit\d$/.test(lower)) return lower.slice(5);
      if (/^numpad\d$/.test(lower)) return `Num${lower.slice(6)}`;
      return k.length === 1 ? k.toUpperCase() : k;
    });
}

/** "CommandOrControl+Alt+B" → "Ctrl+Alt+B". */
export function hotkeyLabel(accelerator: string | null | undefined): string {
  return hotkeyKeys(accelerator).join("+");
}

/** Matches `DEFAULT_HOTKEY` in `src-tauri/src/config.rs`. */
export const DEFAULT_HOTKEY = "CommandOrControl+Alt+B";
