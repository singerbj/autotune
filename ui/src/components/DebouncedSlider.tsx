import { useCallback, useEffect, useId, useRef, useState } from "react";

import { Slider } from "@/components/ui/slider";

export interface DebouncedSliderProps {
  label: string;
  /** Server value (from the query cache). */
  value: number;
  min: number;
  max: number;
  step?: number;
  /** Called at most every `debounceMs` while dragging, and always with the final value. */
  onCommit: (value: number) => void;
  format: (value: number) => string;
  minLabel?: string;
  maxLabel?: string;
  description?: string;
  disabled?: boolean;
  debounceMs?: number;
}

/**
 * A slider that shows its own value while dragging but otherwise renders the
 * server's value. Sends are trailing-debounced (~30 ms) and flushed on release
 * so the final value always reaches the backend.
 */
export function DebouncedSlider({
  label,
  value,
  min,
  max,
  step = 1,
  onCommit,
  format,
  minLabel,
  maxLabel,
  description,
  disabled,
  debounceMs = 30,
}: DebouncedSliderProps) {
  const id = useId();
  const [draft, setDraft] = useState<{ value: number; base: number } | null>(null);
  const [dragging, setDragging] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const pending = useRef<number | null>(null);
  const commitRef = useRef(onCommit);

  useEffect(() => {
    commitRef.current = onCommit;
  }, [onCommit]);

  const flush = useCallback(() => {
    if (timer.current !== null) clearTimeout(timer.current);
    timer.current = null;
    const next = pending.current;
    pending.current = null;
    if (next !== null) commitRef.current(next);
  }, []);

  // Never drop the last value, even if the panel unmounts mid-drag.
  useEffect(() => flush, [flush]);

  // Once the server reports a new value after release, drop the draft.
  if (draft !== null && !dragging && value !== draft.base) setDraft(null);

  const shown = draft === null ? value : draft.value;

  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-baseline justify-between gap-2">
        <label id={`${id}-label`} className="text-sm font-medium">
          {label}
        </label>
        <output
          aria-labelledby={`${id}-label`}
          className="font-mono text-sm text-muted-foreground tabular-nums"
        >
          {format(shown)}
        </output>
      </div>
      <Slider
        aria-labelledby={`${id}-label`}
        thumbLabel={label}
        min={min}
        max={max}
        step={step}
        disabled={disabled}
        value={[shown]}
        onValueChange={([next]) => {
          if (next === undefined) return;
          setDragging(true);
          setDraft((d) => ({ value: next, base: d !== null && dragging ? d.base : value }));
          pending.current = next;
          if (timer.current !== null) clearTimeout(timer.current);
          timer.current = setTimeout(flush, debounceMs);
        }}
        onValueCommit={() => {
          setDragging(false);
          flush();
        }}
      />
      {(minLabel !== undefined || maxLabel !== undefined) && (
        <div className="flex justify-between text-xs text-muted-foreground" aria-hidden>
          <span>{minLabel}</span>
          <span>{maxLabel}</span>
        </div>
      )}
      {description !== undefined && <p className="text-xs text-muted-foreground">{description}</p>}
    </div>
  );
}
