import {
  CircleAlertIcon,
  CircleCheckIcon,
  DownloadIcon,
  LoaderCircleIcon,
  SparklesIcon,
} from "lucide-react";

import type { UpdateStatus } from "@/bindings";

/** FR-22: one line (plus notes) describing the updater's state. */
export function UpdateStatusView({ status }: { status: UpdateStatus | undefined }) {
  if (status === undefined) return null;
  switch (status.state) {
    case "idle":
      return <p className="text-sm text-muted-foreground">Updates haven't been checked yet.</p>;
    case "checking":
      return (
        <p className="flex items-center gap-2 text-sm">
          <LoaderCircleIcon aria-hidden className="size-4 animate-spin" />
          Checking for updates…
        </p>
      );
    case "upToDate":
      return (
        <p className="flex items-center gap-2 text-sm text-success">
          <CircleCheckIcon aria-hidden className="size-4" />
          You're up to date (version {status.current}).
        </p>
      );
    case "downloading": {
      const pct = status.percent === null ? null : Math.round(status.percent);
      return (
        <div className="flex flex-col gap-2">
          <p className="flex items-center gap-2 text-sm">
            <DownloadIcon aria-hidden className="size-4" />
            Downloading version {status.version}
            {pct === null ? "…" : ` — ${pct}%`}
          </p>
          <div
            role="progressbar"
            aria-label="Update download"
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={pct ?? undefined}
            className="h-1.5 overflow-hidden rounded-full bg-muted"
          >
            <div
              className="h-full rounded-full bg-primary transition-[width]"
              style={{ width: `${pct ?? 30}%` }}
            />
          </div>
        </div>
      );
    }
    case "ready":
      return (
        <div className="flex flex-col gap-1">
          <p className="flex items-center gap-2 text-sm font-medium">
            <SparklesIcon aria-hidden className="size-4 text-primary" />
            Version {status.version} is ready to install.
          </p>
          {status.notes !== null && (
            <p className="text-xs whitespace-pre-line text-muted-foreground">{status.notes}</p>
          )}
        </div>
      );
    case "error":
      return (
        <p className="flex items-center gap-2 text-sm text-destructive">
          <CircleAlertIcon aria-hidden className="size-4" />
          Update failed: {status.message}
        </p>
      );
  }
}
