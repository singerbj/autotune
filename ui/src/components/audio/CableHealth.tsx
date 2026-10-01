import { CircleXIcon, LoaderCircleIcon, RotateCwIcon, VolumeXIcon } from "lucide-react";

import type { SetupReport } from "@/bindings";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { useFixPlaybackDevice, useRepairVirtualMic } from "@/lib/queries";

interface CableHealthProps {
  report: SetupReport;
  onRecheck: () => void;
  rechecking: boolean;
}

/**
 * VB-Cable problems with one-click fixes (FR-12, ADR 0012): the cable is
 * missing or turned off, or Windows plays everything into CABLE Input.
 * Renders nothing when the cable is fine.
 */
export function CableHealth({ report, onRecheck, rechecking }: CableHealthProps) {
  const repair = useRepairVirtualMic();
  const fixPlayback = useFixPlaybackDevice();
  const disabled = report.inactiveCables.some((c) => c.disabled);
  const canRepair = !report.remoteSession && (disabled || report.cableRepairAvailable);
  const repairLabel = disabled
    ? "Turn VB-Cable back on"
    : report.inactiveCables.length > 0
      ? "Repair VB-Cable"
      : "Install VB-Cable";

  return (
    <>
      {!report.vbCableInstalled &&
        (repair.data === "restartRequired" ? (
          <Alert variant="warning">
            <RotateCwIcon aria-hidden />
            <div className="flex flex-col gap-1">
              <span className="font-medium">Restart Windows to finish setting up VB-Cable.</span>
              <span>TunedUp picks it up automatically after the restart.</span>
            </div>
          </Alert>
        ) : (
          <Alert variant="warning">
            <CircleXIcon aria-hidden />
            <div className="flex flex-col gap-2">
              {report.remoteSession ? (
                <>
                  <span className="font-medium">You're connected over Remote Desktop.</span>
                  <span>
                    Windows hides this PC's own audio devices in a Remote Desktop session, including
                    VB-Cable, so TunedUp can't see them. Run TunedUp at the PC itself, or in Remote
                    Desktop Connection choose Show Options → Local Resources → Remote audio →
                    Settings → <strong>Play on remote computer</strong> and reconnect.
                  </span>
                </>
              ) : report.inactiveCables.length > 0 ? (
                <>
                  <span className="font-medium">
                    VB-Cable is installed, but Windows has it off.
                  </span>
                  <ul className="list-disc pl-5">
                    {report.inactiveCables.map((c) => (
                      <li key={c.id}>
                        <strong>{c.name}</strong>{" "}
                        {c.disabled ? "is disabled" : "is reported unplugged"} under{" "}
                        {c.isCapture ? "Recording" : "Playback"} devices.
                      </li>
                    ))}
                  </ul>
                  {!canRepair && (
                    <span>
                      Press Win+R, run <strong>mmsys.cpl</strong>, open that tab, right-click the
                      list and tick <strong>Show Disabled Devices</strong>, then right-click the
                      cable and choose <strong>Enable</strong>.
                    </span>
                  )}
                </>
              ) : (
                <>
                  <span className="font-medium">VB-Cable was not found.</span>
                  {canRepair ? (
                    <span>
                      TunedUp can install it now. Windows asks for permission once; a restart is
                      only needed if the cable doesn't come up without one.
                    </span>
                  ) : (
                    <span>
                      The TunedUp installer normally installs it for you. Re-run the installer (or
                      install VB-Cable from vb-audio.com); Windows may need a restart before the
                      cable appears. You can still use monitoring without it.
                    </span>
                  )}
                </>
              )}
              <div className="flex flex-wrap gap-2">
                {canRepair && (
                  <Button size="sm" disabled={repair.isPending} onClick={() => repair.mutate()}>
                    {repair.isPending && (
                      <LoaderCircleIcon aria-hidden className="size-4 animate-spin" />
                    )}
                    {repairLabel}
                  </Button>
                )}
                <Button
                  variant="outline"
                  size="sm"
                  disabled={rechecking || repair.isPending}
                  onClick={onRecheck}
                >
                  Check again
                </Button>
              </div>
            </div>
          </Alert>
        ))}
      {report.playbackOnCable && (
        <Alert variant="warning">
          <VolumeXIcon aria-hidden />
          <div className="flex flex-col gap-2">
            <span className="font-medium">Windows is playing sound into CABLE Input.</span>
            <span>
              Apps that use your default speakers are silent, and their sound goes to anything
              listening on CABLE Output. Windows often does this when VB-Cable is installed.
            </span>
            <Button
              size="sm"
              className="self-start"
              disabled={fixPlayback.isPending}
              onClick={() => fixPlayback.mutate()}
            >
              Switch back to my speakers
            </Button>
          </div>
        </Alert>
      )}
    </>
  );
}
