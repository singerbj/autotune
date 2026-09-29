import { BLACK_KEYS, maskBitCount, maskHas, NOTE_NAMES, toggleMaskBit } from "@/lib/music";
import { cn } from "@/lib/utils";

/**
 * FR-06: 12 piano-style toggles for the custom scale mask (bit n = pitch
 * class n, C = 0). The last enabled note can't be turned off.
 */
export function ScaleKeyboard({
  mask,
  onChange,
  disabled,
}: {
  mask: number;
  onChange: (mask: number) => void;
  disabled?: boolean;
}) {
  const count = maskBitCount(mask);
  return (
    <div role="group" aria-label="Custom scale notes" className="flex flex-col gap-2">
      <div className="flex h-20 items-stretch gap-1">
        {NOTE_NAMES.map((name, pc) => {
          const on = maskHas(mask, pc);
          const black = BLACK_KEYS.has(pc);
          return (
            <button
              key={name}
              type="button"
              aria-pressed={on}
              aria-label={`${name} ${on ? "in scale" : "not in scale"}`}
              title={name}
              disabled={disabled === true || (on && count === 1)}
              onClick={() => onChange(toggleMaskBit(mask, pc))}
              className={cn(
                "flex flex-1 flex-col items-center justify-end rounded-b-md border pb-1.5 text-[11px] font-semibold transition-colors outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50 disabled:cursor-not-allowed",
                black
                  ? "mb-6 border-zinc-700 bg-zinc-800 text-zinc-400"
                  : "border-zinc-300 bg-zinc-100 text-zinc-500",
                on &&
                  (black
                    ? "border-primary bg-primary text-primary-foreground"
                    : "border-primary bg-primary/80 text-primary-foreground"),
              )}
            >
              {name}
            </button>
          );
        })}
      </div>
      <p className="text-xs text-muted-foreground">
        {count} of 12 notes enabled — the tuner snaps only to highlighted notes.
      </p>
    </div>
  );
}
