import { PowerOffIcon, SparklesIcon } from "lucide-react";

import { Button } from "@/components/ui/button";
import { useEngineStatus, useSetBypass } from "@/lib/queries";
import { cn } from "@/lib/utils";

/** FR-10: click-free bypass, kept in sync with the tray/hotkey via events. */
export function BypassToggle() {
  const status = useEngineStatus();
  const setBypass = useSetBypass();
  const bypass = status.data?.bypass ?? false;

  return (
    <Button
      variant={bypass ? "outline" : "default"}
      aria-pressed={bypass}
      aria-label="Bypass"
      title={bypass ? "Tuning is bypassed — click to re-enable" : "Click to hear your dry voice"}
      disabled={status.data === undefined || setBypass.isPending}
      onClick={() => setBypass.mutate(!bypass)}
      className={cn(
        "min-w-32 font-semibold",
        bypass && "border-warning/60 bg-warning/15 text-warning hover:bg-warning/25",
      )}
    >
      {bypass ? <PowerOffIcon aria-hidden /> : <SparklesIcon aria-hidden />}
      Bypass
      <span
        aria-hidden
        className={cn(
          "rounded px-1.5 py-0.5 text-[10px] font-bold tracking-wide uppercase",
          bypass ? "bg-warning/25" : "bg-primary-foreground/20",
        )}
      >
        {bypass ? "On" : "Off"}
      </span>
    </Button>
  );
}
