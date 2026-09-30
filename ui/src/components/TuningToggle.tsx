import { PowerOffIcon, SparklesIcon } from "lucide-react";

import { Button } from "@/components/ui/button";
import { hotkeyLabel } from "@/lib/hotkey";
import { useEngineStatus, useHotkeyStatus, useSetBypass } from "@/lib/queries";
import { cn } from "@/lib/utils";

/**
 * FR-10: turns tuning and headphone monitoring on or off together (off =
 * click-free bypass, headphones silent). Starts off; kept in sync with the
 * tray and the global hotkey via events.
 */
export function TuningToggle() {
  const status = useEngineStatus();
  const hotkey = useHotkeyStatus();
  const setBypass = useSetBypass();
  const on = status.data !== undefined && !status.data.bypass;
  const key = hotkey.data?.active ? ` or press ${hotkeyLabel(hotkey.data.active)}` : "";

  return (
    <Button
      variant={on ? "default" : "outline"}
      aria-pressed={on}
      aria-label="Tuning"
      title={
        on
          ? `Tuning and monitoring are on — click${key} to turn them off`
          : `Tuning and monitoring are off — click${key} to turn them on`
      }
      disabled={status.data === undefined || setBypass.isPending}
      onClick={() => setBypass.mutate(on)}
      className={cn(
        "min-w-32 font-semibold",
        !on && "border-warning/60 bg-warning/15 text-warning hover:bg-warning/25",
      )}
    >
      {on ? <SparklesIcon aria-hidden /> : <PowerOffIcon aria-hidden />}
      Tuning
      <span
        aria-hidden
        className={cn(
          "rounded px-1.5 py-0.5 text-[10px] font-bold tracking-wide uppercase",
          on ? "bg-primary-foreground/20" : "bg-warning/25",
        )}
      >
        {on ? "On" : "Off"}
      </span>
    </Button>
  );
}
