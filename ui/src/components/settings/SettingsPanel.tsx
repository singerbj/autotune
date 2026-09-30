import {
  EyeOffIcon,
  KeyboardIcon,
  PowerIcon,
  RefreshCwIcon,
  RocketIcon,
  WandSparklesIcon,
} from "lucide-react";
import { useCallback, useEffect, useState } from "react";

import { SettingRow } from "@/components/SettingRow";
import { Button } from "@/components/ui/button";
import { Card, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { commands } from "@/lib/api";
import { acceleratorFor, DEFAULT_HOTKEY, hotkeyKeys, hotkeyLabel } from "@/lib/hotkey";
import {
  useAppInfo,
  useCheckForUpdate,
  useConfig,
  useHotkeyStatus,
  useInstallUpdate,
  useLaunchAtLogin,
  useSetConfig,
  useUpdateStatus,
} from "@/lib/queries";
import { errorMessage } from "@/lib/result";
import { toast, toastError } from "@/lib/toast";
import { cn } from "@/lib/utils";

import { UpdateStatusView } from "./UpdateStatusView";

function run(label: string, action: () => Promise<void>): void {
  action().catch((error: unknown) => toastError(label, errorMessage(error)));
}

/**
 * FR-10/FR-20: the global hotkey that turns tuning and monitoring on and off.
 * Click, then press the keys; the new hotkey works right away. Esc cancels.
 */
function HotkeyField({ current }: { current: string }) {
  const [capturing, setCapturing] = useState(false);
  const setConfig = useSetConfig({ inlineError: true });
  const status = useHotkeyStatus();
  const { mutate, reset } = setConfig;

  const save = useCallback(
    (bypassHotkey: string) =>
      mutate(
        { bypassHotkey },
        {
          onSuccess: () =>
            toast({ title: `Hotkey set to ${hotkeyLabel(bypassHotkey)}`, variant: "success" }),
        },
      ),
    [mutate],
  );

  useEffect(() => {
    if (!capturing) return;
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.code === "Escape" && !e.ctrlKey && !e.altKey && !e.shiftKey && !e.metaKey) {
        setCapturing(false);
        return;
      }
      const accelerator = acceleratorFor(e);
      if (accelerator) {
        setCapturing(false);
        save(accelerator);
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [capturing, save]);

  const problem = status.data?.problem;
  return (
    <div className="flex flex-col gap-2">
      <Label id="tuning-hotkey-label">Tuning hotkey</Label>
      <div className="flex gap-2">
        <div
          aria-labelledby="tuning-hotkey-label"
          aria-live="polite"
          className={cn(
            "flex min-h-9 flex-1 items-center gap-1 rounded-md border bg-background px-3 text-sm",
            capturing && "border-primary ring-2 ring-primary/30",
            setConfig.isError && !capturing && "border-destructive",
          )}
        >
          {capturing ? (
            <span className="text-primary">Press a key combination (Esc cancels)</span>
          ) : (
            hotkeyKeys(current).map((key) => (
              <kbd
                key={key}
                className="rounded border bg-muted px-1.5 py-0.5 font-mono text-xs font-semibold"
              >
                {key}
              </kbd>
            ))
          )}
        </div>
        <Button
          variant="secondary"
          aria-describedby="tuning-hotkey-help"
          disabled={setConfig.isPending}
          onClick={() => {
            reset();
            setCapturing((c) => !c);
          }}
        >
          <KeyboardIcon aria-hidden />
          {capturing ? "Cancel" : "Change…"}
        </Button>
        {current !== DEFAULT_HOTKEY && !capturing && (
          <Button
            variant="ghost"
            disabled={setConfig.isPending}
            onClick={() => {
              reset();
              save(DEFAULT_HOTKEY);
            }}
          >
            Reset
          </Button>
        )}
      </div>
      {setConfig.isError ? (
        <p id="tuning-hotkey-help" role="alert" className="text-xs text-destructive">
          {errorMessage(setConfig.error)}
        </p>
      ) : problem ? (
        <p id="tuning-hotkey-help" role="alert" className="text-xs text-warning">
          {problem}
        </p>
      ) : (
        <p id="tuning-hotkey-help" className="text-xs text-muted-foreground">
          Turns tuning and headphone monitoring on or off, even when the window is hidden. Both
          start off each time TunedUp starts.
        </p>
      )}
    </div>
  );
}

function UpdatesCard({ autoUpdate }: { autoUpdate: boolean }) {
  const status = useUpdateStatus();
  const check = useCheckForUpdate();
  const install = useInstallUpdate();
  const setConfig = useSetConfig();
  const state = status.data?.state;
  const busy = state === "checking" || state === "downloading" || check.isPending;

  return (
    <Card>
      <CardHeader>
        <CardTitle>Updates</CardTitle>
      </CardHeader>
      <SettingRow
        htmlFor="auto-update"
        label="Automatic updates"
        description="Download new versions in the background and install on restart."
      >
        <Switch
          id="auto-update"
          checked={autoUpdate}
          onCheckedChange={(v) => setConfig.mutate({ autoUpdate: v })}
        />
      </SettingRow>
      <div aria-live="polite">
        <UpdateStatusView status={status.data} />
      </div>
      <div className="flex gap-2">
        <Button variant="outline" disabled={busy} onClick={() => check.mutate()}>
          <RefreshCwIcon aria-hidden />
          Check for updates
        </Button>
        {state === "ready" && (
          <Button disabled={install.isPending} onClick={() => install.mutate()}>
            <RocketIcon aria-hidden />
            Restart and install
          </Button>
        )}
      </div>
    </Card>
  );
}

/** Settings tab (FR-19, FR-20, FR-22). */
export function SettingsPanel({ onOpenWizard }: { onOpenWizard: () => void }) {
  const config = useConfig();
  const appInfo = useAppInfo();
  const setConfig = useSetConfig();
  const launch = useLaunchAtLogin();

  if (!config.data) return <p className="p-6 text-sm text-muted-foreground">Loading…</p>;
  const c = config.data;

  return (
    <div className="grid grid-cols-2 gap-4">
      <Card>
        <CardHeader>
          <CardTitle>Startup</CardTitle>
        </CardHeader>
        <SettingRow
          htmlFor="launch-at-login"
          label="Launch at login"
          description="Start TunedUp in the tray when you sign in to Windows."
        >
          <Switch
            id="launch-at-login"
            checked={c.launchAtLogin}
            disabled={launch.isPending}
            onCheckedChange={(v) => launch.mutate(v)}
          />
        </SettingRow>
        <SettingRow
          htmlFor="start-on-launch"
          label="Start tuner on launch"
          description="Open the audio devices as soon as the app starts."
        >
          <Switch
            id="start-on-launch"
            checked={c.startEngineOnLaunch}
            onCheckedChange={(v) => setConfig.mutate({ startEngineOnLaunch: v })}
          />
        </SettingRow>
        <HotkeyField current={c.bypassHotkey} />
      </Card>

      <UpdatesCard autoUpdate={c.autoUpdate} />

      <Card>
        <CardHeader>
          <CardTitle>Setup</CardTitle>
          <CardDescription>
            Re-check VB-Cable, Discord and your headset, and measure latency again.
          </CardDescription>
        </CardHeader>
        <Button variant="outline" className="self-start" onClick={onOpenWizard}>
          <WandSparklesIcon aria-hidden />
          Run setup wizard again
        </Button>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>About</CardTitle>
          <CardDescription>
            TunedUp {appInfo.data?.version ?? "…"}
            {appInfo.data && ` · ${appInfo.data.platform}`}
          </CardDescription>
        </CardHeader>
        <div className="flex gap-2">
          <Button
            variant="outline"
            onClick={() => run("Couldn't hide the window", () => commands.hideWindow())}
          >
            <EyeOffIcon aria-hidden />
            Hide to tray
          </Button>
          <Button
            variant="destructive"
            onClick={() => run("Couldn't quit", () => commands.quitApp())}
          >
            <PowerIcon aria-hidden />
            Quit
          </Button>
        </div>
      </Card>
    </div>
  );
}
