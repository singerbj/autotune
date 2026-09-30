import { useMutation, useQuery, useQueryClient, type QueryClient } from "@tanstack/react-query";

import type {
  AppConfig,
  AppInfo,
  ConfigPatch,
  EngineStatusEvent,
  ParamsPatch,
  Preset,
  TuningParams,
  UpdateStatus,
} from "@/bindings";

import { commands } from "./api";
import { unwrap } from "./result";

export const queryKeys = {
  appInfo: ["appInfo"],
  config: ["config"],
  engineStatus: ["engineStatus"],
  devices: ["devices"],
  updateStatus: ["updateStatus"],
  setupCheck: ["setupCheck"],
  hotkeyStatus: ["hotkeyStatus"],
} as const;

// --- cache writers (used by mutations and event listeners) -----------------

export function updateConfigCache(qc: QueryClient, fn: (config: AppConfig) => AppConfig): void {
  qc.setQueryData<AppConfig>(queryKeys.config, (old) => (old ? fn(old) : old));
}

export function setParamsCache(qc: QueryClient, params: TuningParams): void {
  updateConfigCache(qc, (config) => ({ ...config, params }));
}

export function setPresetsCache(qc: QueryClient, presets: Preset[]): void {
  updateConfigCache(qc, (config) => ({ ...config, presets }));
}

export function setBypassCache(qc: QueryClient, bypass: boolean): void {
  qc.setQueryData<EngineStatusEvent>(queryKeys.engineStatus, (old) =>
    old ? { ...old, bypass } : old,
  );
  updateConfigCache(qc, (config) => ({ ...config, params: { ...config.params, bypass } }));
}

export function setEngineStatusCache(qc: QueryClient, status: EngineStatusEvent): void {
  qc.setQueryData(queryKeys.engineStatus, status);
  // The engine is the authority on bypass; mirror it into the params.
  updateConfigCache(qc, (config) =>
    config.params.bypass === status.bypass
      ? config
      : { ...config, params: { ...config.params, bypass: status.bypass } },
  );
}

// --- queries -----------------------------------------------------------------

export function useAppInfo() {
  return useQuery({ queryKey: queryKeys.appInfo, queryFn: () => commands.getAppInfo() });
}

export function useConfig() {
  return useQuery({ queryKey: queryKeys.config, queryFn: () => commands.getConfig() });
}

export function useEngineStatus() {
  return useQuery({
    queryKey: queryKeys.engineStatus,
    queryFn: () => commands.getEngineStatus(),
  });
}

export function useDevices() {
  return useQuery({
    queryKey: queryKeys.devices,
    queryFn: () => unwrap(commands.listDevices()),
  });
}

/** The registered tuning hotkey and why the configured one isn't, if so (FR-10). */
export function useHotkeyStatus() {
  return useQuery({
    queryKey: queryKeys.hotkeyStatus,
    queryFn: () => commands.getHotkeyStatus(),
  });
}

export function useUpdateStatus() {
  return useQuery({
    queryKey: queryKeys.updateStatus,
    queryFn: () => commands.getUpdateStatus(),
  });
}

/** VB-Cable / Discord / warnings report (FR-12, FR-13); optionally polled. */
export function useSetupCheck(pollMs: number | false = false) {
  return useQuery({
    queryKey: queryKeys.setupCheck,
    queryFn: () => unwrap(commands.runSetupCheck()),
    staleTime: 0,
    refetchInterval: pollMs,
  });
}

// --- mutations ---------------------------------------------------------------

export function useSetParams() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (patch: ParamsPatch) => commands.setParams(patch),
    onSuccess: (params) => setParamsCache(qc, params),
    meta: { errorTitle: "Couldn't change tuning" },
  });
}

export function useSetConfig(options: { inlineError?: boolean } = {}) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (patch: ConfigPatch) => unwrap(commands.setConfig(patch)),
    onSuccess: (config, patch) => {
      qc.setQueryData(queryKeys.config, config);
      if (patch.bypassHotkey != null)
        void qc.invalidateQueries({ queryKey: queryKeys.hotkeyStatus });
    },
    meta: { inlineError: options.inlineError ?? false, errorTitle: "Couldn't save setting" },
  });
}

export function useSetBypass() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (bypass: boolean) => commands.setBypass(bypass),
    onSuccess: (bypass) => setBypassCache(qc, bypass),
    meta: { errorTitle: "Couldn't turn tuning on or off" },
  });
}

export function useStartEngine() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => unwrap(commands.startEngine()),
    onSuccess: (status) => setEngineStatusCache(qc, status),
    meta: { errorTitle: "Couldn't start the tuner" },
  });
}

export function useStopEngine() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => commands.stopEngine(),
    onSuccess: (status) => setEngineStatusCache(qc, status),
    meta: { errorTitle: "Couldn't stop the tuner" },
  });
}

export function useRouteAllApps() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (enabled: boolean) => unwrap(commands.setRouteAllApps(enabled)),
    onSuccess: (routeAllApps) => updateConfigCache(qc, (c) => ({ ...c, routeAllApps })),
    meta: { errorTitle: "Couldn't change the Windows default microphone" },
  });
}

export function useLaunchAtLogin() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (enabled: boolean) => unwrap(commands.setLaunchAtLogin(enabled)),
    onSuccess: (launchAtLogin) => updateConfigCache(qc, (c) => ({ ...c, launchAtLogin })),
    meta: { errorTitle: "Couldn't change launch at login" },
  });
}

export function useSavePreset() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (name: string) => unwrap(commands.savePreset(name)),
    onSuccess: (presets) => setPresetsCache(qc, presets),
    meta: { errorTitle: "Couldn't save preset" },
  });
}

export function useLoadPreset() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (name: string) => unwrap(commands.loadPreset(name)),
    onSuccess: (params) => setParamsCache(qc, params),
    meta: { errorTitle: "Couldn't load preset" },
  });
}

export function useDeletePreset() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (name: string) => commands.deletePreset(name),
    onSuccess: (presets) => setPresetsCache(qc, presets),
    meta: { errorTitle: "Couldn't delete preset" },
  });
}

export function useCheckForUpdate() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => unwrap(commands.checkForUpdate()),
    onSuccess: (status) => qc.setQueryData<UpdateStatus>(queryKeys.updateStatus, status),
    meta: { errorTitle: "Update check failed" },
  });
}

export function useInstallUpdate() {
  return useMutation({
    mutationFn: () => unwrap(commands.installUpdate()),
    meta: { errorTitle: "Couldn't install the update" },
  });
}

export function useRunLatencyTest() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => unwrap(commands.runLatencyTest()),
    onSuccess: (result) =>
      updateConfigCache(qc, (c) => ({ ...c, measuredLatencyMs: result.totalMs })),
    meta: { inlineError: true },
  });
}

export function useCompleteWizard() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => commands.completeWizard(),
    onSuccess: (config) => {
      qc.setQueryData(queryKeys.config, config);
      qc.setQueryData<AppInfo>(queryKeys.appInfo, (old) =>
        old ? { ...old, showWizard: false } : old,
      );
    },
    meta: { errorTitle: "Couldn't finish setup" },
  });
}
