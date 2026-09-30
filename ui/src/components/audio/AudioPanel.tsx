import { useMutation } from "@tanstack/react-query";
import { SlidersHorizontalIcon, TriangleAlertIcon } from "lucide-react";

import { DebouncedSlider } from "@/components/DebouncedSlider";
import { SettingRow } from "@/components/SettingRow";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Card, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Switch } from "@/components/ui/switch";
import { commands } from "@/lib/api";
import { formatPercent } from "@/lib/format";
import {
  useAppInfo,
  useConfig,
  useDevices,
  useEngineStatus,
  useRouteAllApps,
  useSetConfig,
} from "@/lib/queries";
import { unwrap } from "@/lib/result";

import { DeviceSelection } from "./DeviceSelection";

/** Audio tab (FR-01, FR-03, FR-04, FR-14). */
export function AudioPanel() {
  const config = useConfig();
  const devices = useDevices();
  const status = useEngineStatus();
  const appInfo = useAppInfo();
  const setConfig = useSetConfig();
  const routeAll = useRouteAllApps();
  const asioPanel = useMutation({
    mutationFn: () => unwrap(commands.openAsioPanel()),
    meta: { errorTitle: "Couldn't open the ASIO control panel" },
  });

  if (!config.data) return <p className="p-6 text-sm text-muted-foreground">Loading…</p>;
  const c = config.data;
  const cableError = status.data?.engine?.cableError ?? null;
  const mic =
    c.captureDevice === null
      ? devices.data?.find((d) => d.direction === "capture" && d.isDefault)
      : devices.data?.find((d) => d.id === c.captureDevice);

  return (
    <div className="grid grid-cols-2 gap-4">
      <Card>
        <CardHeader>
          <CardTitle>Devices</CardTitle>
          <CardDescription>
            A wired headset works best. The list updates when you plug devices in.
          </CardDescription>
        </CardHeader>
        <DeviceSelection />
        {mic?.asioDriver != null && (
          <Button
            variant="outline"
            className="self-start"
            disabled={asioPanel.isPending}
            onClick={() => asioPanel.mutate()}
          >
            <SlidersHorizontalIcon aria-hidden />
            Open ASIO control panel
          </Button>
        )}
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Monitoring</CardTitle>
          <CardDescription>
            Hear the tuned voice in your headphones while tuning is on (the Tuning button or
            hotkey).
          </CardDescription>
        </CardHeader>
        <SettingRow htmlFor="monitor-enabled" label="Monitor in headphones">
          <Switch
            id="monitor-enabled"
            checked={c.monitorEnabled}
            onCheckedChange={(monitorEnabled) => setConfig.mutate({ monitorEnabled })}
          />
        </SettingRow>
        <DebouncedSlider
          label="Monitor volume"
          value={Math.round(c.monitorVolume * 100)}
          min={0}
          max={100}
          disabled={!c.monitorEnabled}
          onCommit={(v) => setConfig.mutate({ monitorVolume: v / 100 })}
          format={(v) => formatPercent(v / 100)}
        />
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Virtual microphone</CardTitle>
          <CardDescription>
            Sends the tuned voice into VB-Cable so Discord and other apps can use it.
          </CardDescription>
        </CardHeader>
        <SettingRow
          htmlFor="virtual-mic"
          label="Virtual mic (VB-Cable)"
          description="Other apps pick “CABLE Output (VB-Audio Virtual Cable)” as their microphone."
        >
          <Switch
            id="virtual-mic"
            checked={c.virtualMicEnabled}
            onCheckedChange={(virtualMicEnabled) => setConfig.mutate({ virtualMicEnabled })}
          />
        </SettingRow>
        <SettingRow
          htmlFor="route-all"
          label="Use for all apps"
          description="Sets CABLE Output as the Windows default microphone so every app hears the tuned voice. Your previous default is restored when you quit."
        >
          <Switch
            id="route-all"
            checked={c.routeAllApps}
            disabled={routeAll.isPending || !c.virtualMicEnabled}
            onCheckedChange={(enabled) => routeAll.mutate(enabled)}
          />
        </SettingRow>
        {cableError !== null && (
          <Alert variant="warning">
            <TriangleAlertIcon aria-hidden />
            <span>{cableError}</span>
          </Alert>
        )}
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Driver mode</CardTitle>
          <CardDescription>
            Lower latency modes are tried first and fall back automatically.
          </CardDescription>
        </CardHeader>
        <SettingRow
          htmlFor="allow-exclusive"
          label="Allow exclusive mode"
          description="Takes the microphone exclusively for the lowest WASAPI latency. Other apps can't use it meanwhile."
        >
          <Switch
            id="allow-exclusive"
            checked={c.allowExclusive}
            onCheckedChange={(allowExclusive) => setConfig.mutate({ allowExclusive })}
          />
        </SettingRow>
        {appInfo.data?.asioCompiled === true && (
          <SettingRow
            htmlFor="allow-asio"
            label="Allow ASIO"
            description="Use the interface's ASIO driver when it has one."
          >
            <Switch
              id="allow-asio"
              checked={c.allowAsio}
              onCheckedChange={(allowAsio) => setConfig.mutate({ allowAsio })}
            />
          </SettingRow>
        )}
      </Card>
    </div>
  );
}
