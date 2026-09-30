import {
  BluetoothIcon,
  CircleCheckIcon,
  CircleDashedIcon,
  CircleXIcon,
  Ear,
  LoaderCircleIcon,
  MicIcon,
  RadioIcon,
} from "lucide-react";
import { useEffect, useRef, type ReactNode } from "react";

import { DeviceSelection } from "@/components/audio/DeviceSelection";
import { LatencyTest } from "@/components/diagnostics/LatencyTest";
import { SettingRow } from "@/components/SettingRow";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import {
  useConfig,
  useEngineStatus,
  useRouteAllApps,
  useSetupCheck,
  useStartEngine,
} from "@/lib/queries";
import { errorMessage } from "@/lib/result";

function Checking() {
  return (
    <p className="flex items-center gap-2 text-sm text-muted-foreground">
      <LoaderCircleIcon aria-hidden className="size-4 animate-spin" />
      Checking…
    </p>
  );
}

export function WelcomeStep() {
  return (
    <div className="flex flex-col gap-3 text-sm leading-relaxed">
      <p>
        TunedUp corrects the pitch of your voice in real time. You'll hear yourself in your
        headphones, and apps like Discord hear the tuned voice through a virtual microphone
        (VB-Cable).
      </p>
      <p className="text-muted-foreground">
        This takes about a minute: pick your devices, check the virtual cable, set up Discord and
        measure latency.
      </p>
    </div>
  );
}

export function DevicesStep() {
  return (
    <div className="flex flex-col gap-4">
      <p className="text-sm text-muted-foreground">
        Choose the microphone you speak into and the headphones you listen with. A wired headset
        gives the lowest latency.
      </p>
      <DeviceSelection />
    </div>
  );
}

export function CableStep() {
  const check = useSetupCheck();
  const config = useConfig();
  const routeAll = useRouteAllApps();
  const report = check.data;

  if (check.isError) {
    return (
      <Alert variant="destructive">
        <CircleXIcon aria-hidden />
        <span>{errorMessage(check.error)}</span>
      </Alert>
    );
  }
  if (!report) return <Checking />;

  return (
    <div className="flex flex-col gap-4">
      {report.vbCableInstalled ? (
        <Alert variant="success">
          <CircleCheckIcon aria-hidden />
          <span>VB-Cable is installed. Other apps can use the tuned voice.</span>
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
                  Desktop Connection choose Show Options → Local Resources → Remote audio → Settings
                  → <strong>Play on remote computer</strong> and reconnect.
                </span>
              </>
            ) : report.inactiveCables.length > 0 ? (
              <>
                <span className="font-medium">VB-Cable is installed, but Windows has it off.</span>
                <ul className="list-disc pl-5">
                  {report.inactiveCables.map((c) => (
                    <li key={`${c.isCapture ? "rec" : "play"}:${c.name}`}>
                      <strong>{c.name}</strong>{" "}
                      {c.disabled ? "is disabled" : "is reported unplugged"} under{" "}
                      {c.isCapture ? "Recording" : "Playback"} devices.
                    </li>
                  ))}
                </ul>
                <span>
                  Press Win+R, run <strong>mmsys.cpl</strong>, open that tab, right-click the list
                  and tick <strong>Show Disabled Devices</strong>, then right-click the cable and
                  choose <strong>Enable</strong>.
                </span>
              </>
            ) : (
              <>
                <span className="font-medium">VB-Cable was not found.</span>
                <span>
                  The TunedUp installer normally installs it for you. Re-run the installer (or
                  install VB-Cable from vb-audio.com); Windows may need a reboot before the cable
                  appears. You can still use monitoring without it.
                </span>
              </>
            )}
            <Button
              variant="outline"
              size="sm"
              className="self-start"
              disabled={check.isFetching}
              onClick={() => void check.refetch()}
            >
              Check again
            </Button>
          </div>
        </Alert>
      )}
      {report.cableConflicts.length > 0 && (
        <Alert variant="warning">
          <RadioIcon aria-hidden />
          <div className="flex flex-col gap-1">
            <span className="font-medium">
              Other apps are already sending audio into CABLE Input:
            </span>
            <ul className="list-disc pl-5">
              {report.cableConflicts.map((app) => (
                <li key={app}>{app}</li>
              ))}
            </ul>
            <span>
              Their sound would mix with your voice. Point them elsewhere, or use VB-Audio's
              additional A+B cables for them.
            </span>
          </div>
        </Alert>
      )}
      {report.vbCableInstalled && config.data && (
        <SettingRow
          htmlFor="wizard-route-all"
          label="Use for all apps"
          description="Makes CABLE Output the Windows default microphone, so every app hears the tuned voice. Restored when you quit."
        >
          <Switch
            id="wizard-route-all"
            checked={config.data.routeAllApps}
            disabled={routeAll.isPending}
            onCheckedChange={(v) => routeAll.mutate(v)}
          />
        </SettingRow>
      )}
    </div>
  );
}

export function WarningsStep() {
  const check = useSetupCheck();
  const report = check.data;
  if (!report)
    return check.isError ? <p className="text-sm">{errorMessage(check.error)}</p> : <Checking />;
  const clean = !report.sidetoneWarning && !report.bluetoothWarning;

  return (
    <div className="flex flex-col gap-4">
      {clean && (
        <Alert variant="success">
          <CircleCheckIcon aria-hidden />
          <span>No problems found with your microphone or headphones.</span>
        </Alert>
      )}
      {report.sidetoneWarning && (
        <Alert variant="warning">
          <Ear aria-hidden />
          <div className="flex flex-col gap-1">
            <span className="font-medium">“Listen to this device” is on for your microphone.</span>
            <span>
              Windows is already playing your raw voice, so you'd hear it twice. Turn it off in
              Sound settings → Recording → your mic → Properties → Listen. Headset "sidetone" or
              "mic monitoring" in the headset's own software doubles your voice the same way.
            </span>
          </div>
        </Alert>
      )}
      {report.bluetoothWarning && (
        <Alert variant="warning">
          <BluetoothIcon aria-hidden />
          <div className="flex flex-col gap-1">
            <span className="font-medium">Bluetooth device selected.</span>
            <span>
              Bluetooth adds 100–200 ms of delay, which makes monitoring your tuned voice
              uncomfortable. A wired (USB or 3.5 mm) headset is strongly recommended.
            </span>
          </div>
        </Alert>
      )}
    </div>
  );
}

function CheckItem({ children }: { children: ReactNode }) {
  return (
    <li className="flex items-start gap-2">
      <CircleDashedIcon aria-hidden className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
      <span>{children}</span>
    </li>
  );
}

export function DiscordStep() {
  // Poll so the check turns green as soon as Discord picks up the cable.
  const check = useSetupCheck(2000);
  const report = check.data;

  return (
    <div className="flex flex-col gap-4 text-sm">
      <p className="text-muted-foreground">In Discord, open User Settings → Voice &amp; Video:</p>
      <ul className="flex flex-col gap-2">
        <CheckItem>
          Set <strong>Input Device</strong> to <strong>Default</strong> (if “Use for all apps” is
          on) or <strong>CABLE Output (VB-Audio Virtual Cable)</strong>.
        </CheckItem>
        <CheckItem>
          Turn off <strong>Noise Suppression</strong> — it fights the tuned voice.
        </CheckItem>
        <CheckItem>
          Turn off <strong>Echo Cancellation</strong>.
        </CheckItem>
        <CheckItem>
          Turn off <strong>Automatic Gain Control</strong>.
        </CheckItem>
      </ul>
      <div aria-live="polite" className="rounded-lg border p-3">
        {report?.discordDetected ? (
          <p className="flex items-center gap-2 font-medium text-success">
            <CircleCheckIcon aria-hidden className="size-4" />
            Discord is using the virtual mic
            {report.discordActive && <span className="font-normal">— listening now</span>}
          </p>
        ) : (
          <p className="flex items-center gap-2 text-muted-foreground">
            <LoaderCircleIcon aria-hidden className="size-4 animate-spin" />
            Waiting for Discord to open the virtual mic…
          </p>
        )}
      </div>
    </div>
  );
}

export function LatencyStep() {
  const status = useEngineStatus();
  const start = useStartEngine();
  const started = useRef(false);
  const state = status.data?.supervisor.state;
  const mutateStart = start.mutate;

  // The test needs running streams; start the engine once if it isn't.
  useEffect(() => {
    if (state === "stopped" && !started.current) {
      started.current = true;
      mutateStart();
    }
  }, [state, mutateStart]);

  return (
    <div className="flex flex-col gap-4">
      <p className="flex items-center gap-2 text-sm">
        <MicIcon aria-hidden className="size-4" />
        {state === "running"
          ? "The tuner is running."
          : start.isPending
            ? "Starting the tuner…"
            : "The tuner isn't running yet."}
      </p>
      <LatencyTest disabled={state !== "running"} />
      <p className="text-xs text-muted-foreground">
        You can skip this and run it later from Diagnostics.
      </p>
    </div>
  );
}

export function DoneStep() {
  return (
    <div className="flex flex-col gap-3 text-sm">
      <p className="flex items-center gap-2 text-base font-medium">
        <CircleCheckIcon aria-hidden className="size-5 text-success" />
        You're all set.
      </p>
      <p className="text-muted-foreground">
        Pick your key and scale on the Tune tab and start singing. Closing the window keeps Voice
        Tuner running in the tray; use the bypass hotkey to hear your dry voice at any time.
      </p>
    </div>
  );
}
