import { EyeOffIcon, PowerIcon, RefreshCwIcon, RocketIcon, WandSparklesIcon } from "lucide-react";
import { useState, type FormEvent } from "react";

import { SettingRow } from "@/components/SettingRow";
import { Button } from "@/components/ui/button";
import { Card, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { commands } from "@/lib/api";
import {
  useAppInfo,
  useCheckForUpdate,
  useConfig,
  useInstallUpdate,
  useLaunchAtLogin,
  useSetConfig,
  useUpdateStatus,
} from "@/lib/queries";
import { errorMessage } from "@/lib/result";
import { toast, toastError } from "@/lib/toast";

import { UpdateStatusView } from "./UpdateStatusView";

function run(label: string, action: () => Promise<void>): void {
  action().catch((error: unknown) => toastError(label, errorMessage(error)));
}

/** FR-20: global bypass hotkey as a Tauri accelerator string. */
function HotkeyField({ current }: { current: string }) {
  const [draft, setDraft] = useState<string | null>(null);
  const setConfig = useSetConfig({ inlineError: true });
  const value = draft ?? current;
  const dirty = draft !== null && draft !== current;

  const onSubmit = (e: FormEvent) => {
    e.preventDefault();
    setConfig.mutate(
      { bypassHotkey: value.trim() },
      {
        onSuccess: () => {
          setDraft(null);
          toast({ title: "Hotkey saved", variant: "success" });
        },
      },
    );
  };

  return (
    <form onSubmit={onSubmit} className="flex flex-col gap-2">
      <Label htmlFor="bypass-hotkey">Bypass hotkey</Label>
      <div className="flex gap-2">
        <Input
          id="bypass-hotkey"
          className="font-mono"
          value={value}
          spellCheck={false}
          aria-invalid={setConfig.isError}
          aria-describedby="bypass-hotkey-help"
          onChange={(e) => {
            setDraft(e.target.value);
            setConfig.reset();
          }}
        />
        <Button type="submit" variant="secondary" disabled={!dirty || setConfig.isPending}>
          Save
        </Button>
      </div>
      {setConfig.isError ? (
        <p id="bypass-hotkey-help" role="alert" className="text-xs text-destructive">
          {errorMessage(setConfig.error)}
        </p>
      ) : (
        <p id="bypass-hotkey-help" className="text-xs text-muted-foreground">
          Works even when the window is hidden, e.g. <code>CommandOrControl+Alt+B</code>.
        </p>
      )}
    </form>
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
          description="Start Voice Tuner in the tray when you sign in to Windows."
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
            Voice Tuner {appInfo.data?.version ?? "…"}
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
