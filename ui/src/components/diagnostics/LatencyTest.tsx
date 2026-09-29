import { HeadphonesIcon, RotateCcwIcon, TimerIcon, TriangleAlertIcon } from "lucide-react";

import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { useRunLatencyTest } from "@/lib/queries";
import { errorMessage } from "@/lib/result";

/** FR-16: acoustic loopback measurement, used by Diagnostics and the wizard. */
export function LatencyTest({ disabled }: { disabled?: boolean }) {
  const test = useRunLatencyTest();

  return (
    <div className="flex flex-col gap-3">
      <p className="flex items-start gap-2 text-sm text-muted-foreground">
        <HeadphonesIcon aria-hidden className="mt-0.5 size-4 shrink-0" />
        Hold an earcup of your headphones against the microphone, stay quiet, then start the test.
        You'll hear a few short clicks.
      </p>
      <div className="flex items-center gap-2">
        <Button
          onClick={() => test.mutate()}
          disabled={disabled === true || test.isPending}
          variant={test.isError ? "outline" : "default"}
        >
          {test.isError ? <RotateCcwIcon aria-hidden /> : <TimerIcon aria-hidden />}
          {test.isPending ? "Measuring…" : test.isError ? "Try again" : "Measure latency"}
        </Button>
      </div>
      <div aria-live="polite">
        {test.data && (
          <dl className="grid grid-cols-3 gap-3 rounded-lg border p-3 text-sm">
            <div>
              <dt className="text-xs text-muted-foreground">Hardware round trip</dt>
              <dd className="font-mono text-lg tabular-nums">
                {test.data.hardwareMs.toFixed(1)} ms
              </dd>
            </div>
            <div>
              <dt className="text-xs text-muted-foreground">Total (with tuning)</dt>
              <dd className="font-mono text-lg tabular-nums">{test.data.totalMs.toFixed(1)} ms</dd>
            </div>
            <div>
              <dt className="text-xs text-muted-foreground">Confidence</dt>
              <dd className="font-mono text-lg tabular-nums">
                {Math.round(test.data.confidence * 100)}%
                {test.data.confidence < 0.5 && (
                  <span className="ml-1 text-xs text-warning">(low — try again)</span>
                )}
              </dd>
            </div>
          </dl>
        )}
        {test.isError && (
          <Alert variant="destructive">
            <TriangleAlertIcon aria-hidden />
            <span>{errorMessage(test.error)}</span>
          </Alert>
        )}
      </div>
    </div>
  );
}
