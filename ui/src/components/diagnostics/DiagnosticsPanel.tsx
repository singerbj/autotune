import { CopyIcon } from "lucide-react";

import type { StreamInfo } from "@/bindings";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { commands } from "@/lib/api";
import { framesToMs, formatMs, SUPERVISOR_LABELS, tierLabel } from "@/lib/format";
import { useMeterSnapshot } from "@/lib/meters";
import { useEngineStatus } from "@/lib/queries";
import { errorMessage } from "@/lib/result";
import { toast, toastError } from "@/lib/toast";
import { cn } from "@/lib/utils";

import { LatencyTest } from "./LatencyTest";

function StreamCard({ title, stream }: { title: string; stream: StreamInfo | null }) {
  return (
    <Card className="gap-3">
      <CardHeader>
        <CardTitle>{title}</CardTitle>
      </CardHeader>
      {stream === null ? (
        <p className="text-sm text-muted-foreground">Not open.</p>
      ) : (
        <>
          <p className="truncate text-sm font-medium" title={stream.deviceName}>
            {stream.deviceName}
          </p>
          <Badge variant="secondary">{tierLabel(stream.tier)}</Badge>
          <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-xs">
            <dt className="text-muted-foreground">Sample rate</dt>
            <dd className="font-mono tabular-nums">{stream.sampleRate} Hz</dd>
            <dt className="text-muted-foreground">Channels</dt>
            <dd className="font-mono tabular-nums">{stream.channels}</dd>
            <dt className="text-muted-foreground">Period</dt>
            <dd className="font-mono tabular-nums">
              {stream.periodFrames} frames ·{" "}
              {formatMs(framesToMs(stream.periodFrames, stream.sampleRate), 2)}
            </dd>
            <dt className="text-muted-foreground">Stream latency</dt>
            <dd className="font-mono tabular-nums">
              {stream.streamLatencyFrames} frames ·{" "}
              {formatMs(framesToMs(stream.streamLatencyFrames, stream.sampleRate), 2)}
            </dd>
          </dl>
          {stream.fallbacks.length > 0 && (
            <div className="flex flex-col gap-1">
              <p className="text-xs font-medium text-muted-foreground">Fallbacks</p>
              <ul className="flex flex-col gap-1 text-xs">
                {stream.fallbacks.map((f) => (
                  <li key={f.tier} className="rounded border px-2 py-1">
                    <span className="font-medium">{tierLabel(f.tier)}</span>
                    <span className="text-muted-foreground"> — {f.error}</span>
                  </li>
                ))}
              </ul>
            </div>
          )}
        </>
      )}
    </Card>
  );
}

function TimingBar({ label, us, budgetUs }: { label: string; us: number; budgetUs: number }) {
  const frac = budgetUs > 0 ? Math.min(1, us / budgetUs) : 0;
  return (
    <div className="flex flex-col gap-1">
      <div className="flex justify-between text-xs">
        <span className="text-muted-foreground">{label}</span>
        <span className="font-mono tabular-nums">
          {Math.round(us)} / {Math.round(budgetUs)} µs
        </span>
      </div>
      <div
        role="meter"
        aria-label={`${label} callback time`}
        aria-valuemin={0}
        aria-valuemax={Math.round(budgetUs)}
        aria-valuenow={Math.round(us)}
        className="h-2 overflow-hidden rounded-full bg-muted"
      >
        <div
          className={cn(
            "h-full rounded-full",
            frac > 0.8 ? "bg-destructive" : frac > 0.5 ? "bg-warning" : "bg-success",
          )}
          style={{ width: `${frac * 100}%` }}
        />
      </div>
    </div>
  );
}

type CounterKey =
  | "xruns"
  | "captureDiscontinuities"
  | "monitorUnderruns"
  | "cableUnderruns"
  | "ringOverflows";

const COUNTERS: ReadonlyArray<{ key: CounterKey; label: string }> = [
  { key: "xruns", label: "Xruns" },
  { key: "captureDiscontinuities", label: "Capture discontinuities" },
  { key: "monitorUnderruns", label: "Monitor underruns" },
  { key: "cableUnderruns", label: "Cable underruns" },
  { key: "ringOverflows", label: "Ring overflows" },
];

function MetersCard() {
  const m = useMeterSnapshot(250);
  return (
    <Card className="gap-3">
      <CardHeader>
        <CardTitle>Health</CardTitle>
        <CardDescription>Counters since the engine started.</CardDescription>
      </CardHeader>
      {m === null ? (
        <p className="text-sm text-muted-foreground">No meter data — is the tuner running?</p>
      ) : (
        <>
          <dl className="grid grid-cols-[repeat(auto-fill,minmax(7.5rem,1fr))] gap-2">
            {COUNTERS.map(({ key, label }) => {
              const n = m[key];
              return (
                <div key={key} className="rounded-md border p-2">
                  <dt className="text-[11px] leading-tight text-muted-foreground">{label}</dt>
                  <dd className={cn("font-mono text-lg tabular-nums", n > 0 && "text-warning")}>
                    {n}
                  </dd>
                </div>
              );
            })}
          </dl>
          <TimingBar label="p99" us={m.callbackP99Us} budgetUs={m.callbackBudgetUs} />
          <TimingBar label="max" us={m.callbackMaxUs} budgetUs={m.callbackBudgetUs} />
          <dl className="grid grid-cols-3 gap-2 text-xs">
            <div>
              <dt className="text-muted-foreground">Monitor buffer</dt>
              <dd className="font-mono tabular-nums">{formatMs(m.monitorFillMs)}</dd>
            </div>
            <div>
              <dt className="text-muted-foreground">Cable buffer</dt>
              <dd className="font-mono tabular-nums">{formatMs(m.cableFillMs)}</dd>
            </div>
            <div>
              <dt className="text-muted-foreground">Cable resampler ratio</dt>
              <dd className="font-mono tabular-nums">{m.cableRatio.toFixed(6)}</dd>
            </div>
          </dl>
        </>
      )}
    </Card>
  );
}

async function copyDiagnostics(): Promise<void> {
  try {
    const diagnostics = await commands.getDiagnostics();
    await navigator.clipboard.writeText(diagnostics.report);
    toast({
      title: "Diagnostics copied",
      description: "Paste them into your support request.",
      variant: "success",
    });
  } catch (error) {
    toastError("Couldn't copy diagnostics", errorMessage(error));
  }
}

/** Diagnostics tab (FR-16, FR-18). */
export function DiagnosticsPanel() {
  const status = useEngineStatus();
  const engine = status.data?.engine ?? null;
  const supervisor = status.data?.supervisor;

  return (
    <div className="flex flex-col gap-4">
      <div className="flex items-center justify-between gap-4">
        <p className="text-sm text-muted-foreground">
          {supervisor
            ? `Engine ${SUPERVISOR_LABELS[supervisor.state].toLowerCase()} · ${supervisor.restarts} restart${supervisor.restarts === 1 ? "" : "s"}${supervisor.usingFallbackDevice ? " · using a fallback device" : ""}`
            : "Loading…"}
          {engine &&
            ` · DSP ${formatMs(engine.dspLatencyMs)} · estimated total ${formatMs(engine.estimatedLatencyMs)}${engine.monitorResampling ? " · monitor resampling" : ""}`}
        </p>
        <Button variant="outline" onClick={() => void copyDiagnostics()}>
          <CopyIcon aria-hidden />
          Copy diagnostics
        </Button>
      </div>
      <div className="grid grid-cols-3 gap-4">
        <StreamCard title="Capture" stream={engine?.capture ?? null} />
        <StreamCard title="Monitor" stream={engine?.monitor ?? null} />
        <StreamCard title="Virtual mic (cable)" stream={engine?.cable ?? null} />
      </div>
      <div className="grid grid-cols-2 gap-4">
        <MetersCard />
        <Card className="gap-3">
          <CardHeader>
            <CardTitle>Measure latency</CardTitle>
            <CardDescription>
              Plays clicks and listens for them to measure the real delay.
            </CardDescription>
          </CardHeader>
          <LatencyTest disabled={supervisor?.state !== "running"} />
        </Card>
      </div>
    </div>
  );
}
