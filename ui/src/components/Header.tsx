import { DownloadIcon, PlayIcon, SquareIcon, TimerIcon } from "lucide-react";

import type { SupervisorState } from "@/bindings";
import { BypassToggle } from "@/components/BypassToggle";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Tooltip } from "@/components/ui/tooltip";
import { SUPERVISOR_LABELS, tierLabel } from "@/lib/format";
import {
  useConfig,
  useEngineStatus,
  useInstallUpdate,
  useStartEngine,
  useStopEngine,
  useUpdateStatus,
} from "@/lib/queries";

const STATE_VARIANT: Record<SupervisorState, "success" | "warning" | "outline"> = {
  running: "success",
  recovering: "warning",
  stopped: "outline",
};

function EngineButton({ state }: { state: SupervisorState | undefined }) {
  const start = useStartEngine();
  const stop = useStopEngine();
  const busy = start.isPending || stop.isPending;
  if (state === "running" || state === "recovering") {
    return (
      <Button variant="secondary" disabled={busy} onClick={() => stop.mutate()}>
        <SquareIcon aria-hidden />
        Stop
      </Button>
    );
  }
  return (
    <Button variant="success" disabled={busy || state === undefined} onClick={() => start.mutate()}>
      <PlayIcon aria-hidden />
      {start.isPending ? "Starting…" : "Start"}
    </Button>
  );
}

export function Header() {
  const status = useEngineStatus();
  const config = useConfig();
  const update = useUpdateStatus();
  const install = useInstallUpdate();

  const supervisor = status.data?.supervisor;
  const engine = status.data?.engine ?? null;
  const measured = config.data?.measuredLatencyMs ?? null;
  const ready = update.data?.state === "ready" ? update.data : null;

  return (
    <header className="flex min-h-16 shrink-0 items-center gap-4 border-b bg-card/60 px-5 py-2">
      <div className="flex items-center gap-2">
        <img src="/icon.svg" alt="" className="size-7" />
        <h1 className="text-base font-semibold tracking-tight">TunedUp</h1>
      </div>

      <div className="flex min-w-0 flex-1 flex-wrap items-center gap-2" aria-label="Engine status">
        {supervisor && (
          <Tooltip content={supervisor.lastError}>
            <Badge
              variant={STATE_VARIANT[supervisor.state]}
              tabIndex={supervisor.lastError ? 0 : undefined}
              aria-label={`Engine ${SUPERVISOR_LABELS[supervisor.state]}${supervisor.lastError ? `: ${supervisor.lastError}` : ""}`}
            >
              <span
                aria-hidden
                className="size-1.5 rounded-full bg-current data-[pulse=true]:animate-pulse"
                data-pulse={supervisor.state === "recovering"}
              />
              {SUPERVISOR_LABELS[supervisor.state]}
            </Badge>
          </Tooltip>
        )}
        {engine && (
          <Badge variant="secondary" title="Capture backend">
            {tierLabel(engine.capture.tier)}
          </Badge>
        )}
        {engine && (
          <Badge variant="outline" title="Estimated end-to-end latency">
            <TimerIcon aria-hidden />~{engine.estimatedLatencyMs.toFixed(0)} ms
            {measured !== null && (
              <span className="text-muted-foreground">· measured {measured.toFixed(0)} ms</span>
            )}
          </Badge>
        )}
      </div>

      <div className="flex items-center gap-2">
        {ready && (
          <Button
            variant="outline"
            size="sm"
            disabled={install.isPending}
            onClick={() => install.mutate()}
            title={ready.notes ?? `Version ${ready.version} is ready to install`}
          >
            <DownloadIcon aria-hidden />
            Update ready — restart
          </Button>
        )}
        <BypassToggle />
        <EngineButton state={supervisor?.state} />
      </div>
    </header>
  );
}
