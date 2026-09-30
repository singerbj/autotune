/**
 * Stateful in-memory stand-in for the Rust backend, used in a plain browser
 * and in Vitest. It is typed against the generated bindings (`typeof commands`
 * / `typeof events`), so it cannot drift from the real API.
 */
import type { Event, EventCallback, UnlistenFn } from "@tauri-apps/api/event";

import type {
  AppConfig,
  AppError,
  BackendTier,
  commands as TauriCommands,
  DeviceInfo,
  Diagnostics,
  EngineStatus,
  EngineStatusEvent,
  events as TauriEvents,
  MeterSnapshot,
  Preset,
  SetupReport,
  StreamInfo,
  SupervisorStatus,
  TuningParams,
  UpdateStatus,
} from "@/bindings";

import { tierLabel, VOICE_RANGES } from "./format";
import { hzToMidi, midiToHz, nearestTargetMidi, scaleMask } from "./music";

type Commands = typeof TauriCommands;
type Events = typeof TauriEvents;

export interface MockEvent<T> {
  (target: unknown): MockEventBase<T>;
  listen: (cb: EventCallback<T>) => Promise<UnlistenFn>;
  once: (cb: EventCallback<T>) => Promise<UnlistenFn>;
  emit: (payload: T) => Promise<void>;
  listenerCount: () => number;
}

interface MockEventBase<T> {
  listen: (cb: EventCallback<T>) => Promise<UnlistenFn>;
  once: (cb: EventCallback<T>) => Promise<UnlistenFn>;
  emit: (payload: T) => Promise<void>;
}

export interface MockBackend {
  commands: Commands;
  events: Events;
  /** Restore the initial state and stop all timers (tests). */
  reset: () => void;
  /** Replace the device list and fire `devicesChangedEvent`. */
  setDevices: (devices: DeviceInfo[]) => void;
}

// Delays are real in the browser demo and zero under Vitest.
const SPEED = import.meta.env.MODE === "test" ? 0 : 1;

let eventId = 1;

function makeMockEvent<T>(name: string, onListenersChanged?: () => void): MockEvent<T> {
  const callbacks = new Set<EventCallback<T>>();
  const listen = (cb: EventCallback<T>): Promise<UnlistenFn> => {
    // Wrap so the same callback can be registered twice and removed independently.
    const wrapped: EventCallback<T> = (event) => cb(event);
    callbacks.add(wrapped);
    onListenersChanged?.();
    return Promise.resolve(() => {
      callbacks.delete(wrapped);
      onListenersChanged?.();
    });
  };
  const once = (cb: EventCallback<T>): Promise<UnlistenFn> => {
    const wrapped: EventCallback<T> = (event) => {
      callbacks.delete(wrapped);
      onListenersChanged?.();
      cb(event);
    };
    callbacks.add(wrapped);
    onListenersChanged?.();
    return Promise.resolve(() => {
      callbacks.delete(wrapped);
      onListenersChanged?.();
    });
  };
  const emit = (payload: T): Promise<void> => {
    const event: Event<T> = { event: name, id: eventId++, payload };
    for (const cb of callbacks) cb(event);
    return Promise.resolve();
  };
  const base: MockEventBase<T> = { listen, once, emit };
  return Object.assign((_target: unknown) => base, {
    ...base,
    listenerCount: () => callbacks.size,
  });
}

const DEVICES: DeviceInfo[] = [
  {
    id: "{0.0.1.00000000}.{a1-headset-mic}",
    name: "Headset Microphone (Arctis 7)",
    direction: "capture",
    isDefault: true,
    isDefaultCommunications: true,
    isVbCable: false,
    isBluetooth: false,
    asioDriver: null,
    containerId: "{arctis-7}",
  },
  {
    id: "{0.0.1.00000000}.{b2-focusrite}",
    name: "Analogue 1 + 2 (Focusrite USB Audio)",
    direction: "capture",
    isDefault: false,
    isDefaultCommunications: false,
    isVbCable: false,
    isBluetooth: false,
    asioDriver: "Focusrite USB ASIO",
    containerId: "{focusrite}",
  },
  {
    id: "{0.0.1.00000000}.{c3-airpods}",
    name: "Headset (AirPods Pro Hands-Free)",
    direction: "capture",
    isDefault: false,
    isDefaultCommunications: false,
    isVbCable: false,
    isBluetooth: true,
    asioDriver: null,
    containerId: "{airpods}",
  },
  {
    id: "{0.0.1.00000000}.{d4-cable-output}",
    name: "CABLE Output (VB-Audio Virtual Cable)",
    direction: "capture",
    isDefault: false,
    isDefaultCommunications: false,
    isVbCable: true,
    isBluetooth: false,
    asioDriver: null,
    containerId: "{vb-cable}",
  },
  {
    id: "{0.0.0.00000000}.{e5-headphones}",
    name: "Headphones (Arctis 7)",
    direction: "render",
    isDefault: true,
    isDefaultCommunications: true,
    isVbCable: false,
    isBluetooth: false,
    asioDriver: null,
    containerId: "{arctis-7}",
  },
  {
    id: "{0.0.0.00000000}.{f6-speakers}",
    name: "Speakers (Realtek(R) Audio)",
    direction: "render",
    isDefault: false,
    isDefaultCommunications: false,
    isVbCable: false,
    isBluetooth: false,
    asioDriver: null,
    containerId: "{realtek}",
  },
  {
    id: "{0.0.0.00000000}.{g7-airpods}",
    name: "Headphones (AirPods Pro)",
    direction: "render",
    isDefault: false,
    isDefaultCommunications: false,
    isVbCable: false,
    isBluetooth: true,
    asioDriver: null,
    containerId: "{airpods}",
  },
  {
    id: "{0.0.0.00000000}.{h8-cable-input}",
    name: "CABLE Input (VB-Audio Virtual Cable)",
    direction: "render",
    isDefault: false,
    isDefaultCommunications: false,
    isVbCable: true,
    isBluetooth: false,
    asioDriver: null,
    containerId: "{vb-cable}",
  },
];

const DEFAULT_PARAMS: TuningParams = {
  key: 0,
  scale: "major",
  // C major as a starting point for the custom mask.
  customMask: 0xab5,
  retuneMs: 40,
  humanize: 0.25,
  voiceRange: "mid",
  mix: 1,
  gateThresholdDb: -55,
  bypass: false,
};

function initialConfig(): AppConfig {
  return {
    schemaVersion: 1,
    params: { ...DEFAULT_PARAMS },
    captureDevice: null,
    monitorDevice: null,
    monitorEnabled: true,
    monitorVolume: 0.8,
    virtualMicEnabled: true,
    allowExclusive: true,
    allowAsio: true,
    routeAllApps: false,
    routingBackup: null,
    wizardCompleted: true,
    launchAtLogin: false,
    startEngineOnLaunch: true,
    bypassHotkey: "CommandOrControl+Alt+B",
    autoUpdate: true,
    presets: [
      { name: "Hard tune", params: { ...DEFAULT_PARAMS, retuneMs: 0, humanize: 0 } },
      { name: "Subtle", params: { ...DEFAULT_PARAMS, retuneMs: 120, humanize: 0.6, mix: 0.8 } },
    ],
    measuredLatencyMs: null,
  };
}

interface MockState {
  config: AppConfig;
  devices: DeviceInfo[];
  supervisor: SupervisorStatus;
  update: UpdateStatus;
  setupChecks: number;
  counters: {
    xruns: number;
    captureDiscontinuities: number;
    monitorUnderruns: number;
    cableUnderruns: number;
    ringOverflows: number;
  };
  startedAt: number;
}

function initialState(): MockState {
  return {
    config: initialConfig(),
    devices: DEVICES.map((d) => ({ ...d })),
    supervisor: { state: "running", lastError: null, restarts: 0, usingFallbackDevice: false },
    update: { state: "idle" },
    setupChecks: 0,
    counters: {
      xruns: 0,
      captureDiscontinuities: 0,
      monitorUnderruns: 0,
      cableUnderruns: 0,
      ringOverflows: 0,
    },
    startedAt: Date.now(),
  };
}

const APP_VERSION = "0.1.0";
const SAMPLE_RATE = 48_000;

function ok<T>(data: T): { status: "ok"; data: T } {
  return { status: "ok", data };
}

function fail(kind: AppError["kind"], message: string): { status: "error"; error: AppError } {
  return { status: "error", error: { kind, message } };
}

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms * SPEED));
}

const MODIFIERS = new Set([
  "commandorcontrol",
  "cmdorctrl",
  "control",
  "ctrl",
  "alt",
  "option",
  "shift",
  "super",
  "meta",
  "command",
  "cmd",
]);

/** Mirrors the accelerator parser closely enough for the demo. */
function validateHotkey(accelerator: string): string | null {
  const parts = accelerator.split("+").map((p) => p.trim());
  const key = parts.at(-1) ?? "";
  const mods = parts.slice(0, -1);
  if (accelerator.trim() === "") return "The bypass hotkey can't be empty.";
  if (mods.length === 0) return `"${accelerator}" needs at least one modifier (e.g. Ctrl+Alt+B).`;
  const badMod = mods.find((m) => !MODIFIERS.has(m.toLowerCase()));
  if (badMod !== undefined) return `Unknown modifier "${badMod}" in "${accelerator}".`;
  if (!/^([A-Za-z0-9]|F([1-9]|1[0-9]|2[0-4])|Space|Tab|Home|End|Insert|Delete)$/.test(key)) {
    return `Unknown key "${key}" in "${accelerator}".`;
  }
  return null;
}

function stream(
  device: DeviceInfo | undefined,
  tier: BackendTier,
  periodFrames: number,
  streamLatencyFrames: number,
  channels: number,
  fallbacks: StreamInfo["fallbacks"] = [],
): StreamInfo {
  return {
    deviceId: device?.id ?? "default",
    deviceName: device?.name ?? "System default",
    tier,
    sampleRate: SAMPLE_RATE,
    channels,
    periodFrames,
    streamLatencyFrames,
    fallbacks,
  };
}

const framesMs = (frames: number) => (frames / SAMPLE_RATE) * 1000;

export function createMockBackend(): MockBackend {
  let state = initialState();
  let meterTimer: ReturnType<typeof setInterval> | null = null;
  const timers = new Set<ReturnType<typeof setTimeout>>();

  const after = (ms: number, fn: () => void): void => {
    const id = setTimeout(() => {
      timers.delete(id);
      fn();
    }, ms * SPEED);
    timers.add(id);
  };

  const findDevice = (id: string | null, direction: DeviceInfo["direction"]) =>
    (id === null ? undefined : state.devices.find((d) => d.id === id)) ??
    state.devices.find((d) => d.direction === direction && d.isDefault && !d.isVbCable);

  const selectedMic = () => findDevice(state.config.captureDevice, "capture");
  const selectedHeadphones = () => findDevice(state.config.monitorDevice, "render");

  function engineStatus(): EngineStatus | null {
    if (state.supervisor.state !== "running") return null;
    const { config } = state;
    const mic = selectedMic();
    const asio = config.allowAsio && mic?.asioDriver != null;
    const captureTier: BackendTier = asio
      ? "asio"
      : config.allowExclusive
        ? "wasapiExclusive"
        : "wasapiSharedLowLatency";
    const captureFallbacks: StreamInfo["fallbacks"] =
      config.allowAsio && !asio
        ? [{ tier: "asio", error: "No ASIO driver owns this endpoint." }]
        : [];
    const capture = stream(mic, captureTier, asio ? 64 : 128, asio ? 32 : 96, 1, captureFallbacks);
    const monitor = stream(selectedHeadphones(), "wasapiSharedLowLatency", 144, 120, 2, [
      {
        tier: "wasapiExclusive",
        error: "Device is in use by another application (AUDCLNT_E_DEVICE_IN_USE).",
      },
    ]);
    const cableInput = state.devices.find((d) => d.isVbCable && d.direction === "render");
    const cable =
      config.virtualMicEnabled && cableInput ? stream(cableInput, "wasapiShared", 480, 0, 2) : null;
    const cableError =
      config.virtualMicEnabled && !cableInput
        ? "VB-Cable is not installed, so other apps can't hear the tuned voice."
        : null;
    const dspLatencyMs =
      VOICE_RANGES.find((r) => r.value === config.params.voiceRange)?.latencyMs ?? 10;
    const estimatedLatencyMs =
      framesMs(capture.periodFrames + capture.streamLatencyFrames) +
      dspLatencyMs +
      2 +
      framesMs(monitor.periodFrames + monitor.streamLatencyFrames);
    return {
      capture,
      monitor,
      cable,
      cableError,
      monitorResampling: false,
      dspLatencyMs,
      estimatedLatencyMs,
    };
  }

  const status = (): EngineStatusEvent => ({
    supervisor: { ...state.supervisor },
    engine: engineStatus(),
    bypass: state.config.params.bypass,
  });

  const setupReport = (): SetupReport => {
    const cableInput = state.devices.find((d) => d.isVbCable && d.direction === "render");
    const cableOutput = state.devices.find((d) => d.isVbCable && d.direction === "capture");
    return {
      vbCableInstalled: cableInput !== undefined && cableOutput !== undefined,
      cableInputId: cableInput?.id ?? null,
      cableOutputId: cableOutput?.id ?? null,
      inactiveCables: [],
      cableConflicts: ["obs64.exe"],
      discordDetected: state.setupChecks >= 3,
      discordActive: state.setupChecks >= 5,
      bluetoothWarning:
        (selectedMic()?.isBluetooth ?? false) || (selectedHeadphones()?.isBluetooth ?? false),
      sidetoneWarning: false,
      remoteSession: false,
    };
  };

  // --- meters (FR-17) -----------------------------------------------------

  function meterSnapshot(): MeterSnapshot {
    const t = (Date.now() - state.startedAt) / 1000;
    const { params } = state.config;
    const phrase = t % 3.2;
    const voiced = phrase < 2.3;
    const envelope = voiced ? Math.sin((phrase / 2.3) * Math.PI) : 0;
    const inputDb = voiced ? -30 + 20 * envelope + 2 * Math.sin(t * 9) : -58 + 3 * Math.random();
    const gateOpen = params.gateThresholdDb <= -100 || inputDb > params.gateThresholdDb;
    // A slow glide across a few semitones around A3 with light vibrato.
    const detectedMidi = 57 + 3.2 * Math.sin(t * 0.45) + 0.2 * Math.sin(t * 34);
    const detectedHz = voiced && gateOpen ? midiToHz(detectedMidi) : 0;
    const mask = scaleMask(params.scale, params.key, params.customMask);
    const midi = hzToMidi(detectedHz);
    const targetMidi = midi === null || params.bypass ? -1 : nearestTargetMidi(midi, mask);
    const correctionCents =
      midi === null || targetMidi < 0 ? 0 : (targetMidi - midi) * 100 * params.mix;
    if (Math.random() < 0.002) state.counters.monitorUnderruns++;
    if (Math.random() < 0.0005) state.counters.xruns++;
    const cableOn = state.config.virtualMicEnabled;
    return {
      dsp: {
        inputDb,
        detectedHz,
        targetMidi,
        correctionCents,
        voiced: voiced && gateOpen,
        gateOpen,
      },
      monitorFillMs: 5.5 + Math.sin(t * 3) * 0.8,
      cableFillMs: cableOn ? 20 + Math.sin(t * 0.7) * 2 : 0,
      cableRatio: cableOn ? 1 + Math.sin(t * 0.1) * 0.00004 : 1,
      ...state.counters,
      callbackP99Us: 410 + Math.sin(t) * 30,
      callbackMaxUs: 960 + Math.sin(t * 0.3) * 120,
      callbackBudgetUs: Math.round((128 / SAMPLE_RATE) * 1e6),
    };
  }

  const syncMeterLoop = (): void => {
    const shouldRun = state.supervisor.state === "running" && metersEvent.listenerCount() > 0;
    if (shouldRun && meterTimer === null) {
      meterTimer = setInterval(() => void metersEvent.emit(meterSnapshot()), 33);
    } else if (!shouldRun && meterTimer !== null) {
      clearInterval(meterTimer);
      meterTimer = null;
    }
  };

  const bypassEvent = makeMockEvent<boolean>("bypass-event");
  const devicesChangedEvent = makeMockEvent<DeviceInfo[]>("devices-changed-event");
  const engineStatusEvent = makeMockEvent<EngineStatusEvent>("engine-status-event");
  const errorEvent = makeMockEvent<string>("error-event");
  const metersEvent = makeMockEvent<MeterSnapshot>("meters-event", () => syncMeterLoop());
  const updateEvent = makeMockEvent<UpdateStatus>("update-event");

  const events: Events = {
    bypassEvent,
    devicesChangedEvent,
    engineStatusEvent,
    errorEvent,
    metersEvent,
    updateEvent,
  };

  const publishStatus = (): void => {
    syncMeterLoop();
    void engineStatusEvent.emit(status());
  };

  const setUpdate = (next: UpdateStatus): void => {
    state.update = next;
    void updateEvent.emit(next);
  };

  const presetsCopy = (): Preset[] => state.config.presets.map((p) => ({ ...p }));

  const commands: Commands = {
    ping: (message) => Promise.resolve(`pong: ${message}`),

    getAppInfo: () =>
      Promise.resolve({
        version: APP_VERSION,
        platform: "windows (browser mock)",
        showWizard:
          !state.config.wizardCompleted ||
          (typeof location !== "undefined" &&
            new URLSearchParams(location.search).has("first-run")),
        asioCompiled: true,
      }),

    listDevices: () => Promise.resolve(ok(state.devices.map((d) => ({ ...d })))),

    getConfig: () => Promise.resolve(structuredClone(state.config)),

    setConfig: async (patch) => {
      if (patch.bypassHotkey != null) {
        const problem = validateHotkey(patch.bypassHotkey);
        if (problem !== null) return fail("config", problem);
      }
      const next: AppConfig = { ...state.config };
      if (patch.captureDevice !== undefined) next.captureDevice = patch.captureDevice;
      if (patch.monitorDevice !== undefined) next.monitorDevice = patch.monitorDevice;
      if (patch.monitorEnabled != null) next.monitorEnabled = patch.monitorEnabled;
      if (patch.monitorVolume != null) next.monitorVolume = patch.monitorVolume;
      if (patch.virtualMicEnabled != null) next.virtualMicEnabled = patch.virtualMicEnabled;
      if (patch.allowExclusive != null) next.allowExclusive = patch.allowExclusive;
      if (patch.allowAsio != null) next.allowAsio = patch.allowAsio;
      if (patch.startEngineOnLaunch != null) next.startEngineOnLaunch = patch.startEngineOnLaunch;
      if (patch.bypassHotkey != null) next.bypassHotkey = patch.bypassHotkey;
      if (patch.autoUpdate != null) next.autoUpdate = patch.autoUpdate;
      const rebuild =
        next.captureDevice !== state.config.captureDevice ||
        next.monitorDevice !== state.config.monitorDevice ||
        next.virtualMicEnabled !== state.config.virtualMicEnabled ||
        next.allowExclusive !== state.config.allowExclusive ||
        next.allowAsio !== state.config.allowAsio;
      state.config = next;
      if (rebuild) {
        await delay(150);
        publishStatus();
      }
      return ok(structuredClone(state.config));
    },

    startEngine: async () => {
      await delay(250);
      state.supervisor = { ...state.supervisor, state: "running", lastError: null };
      publishStatus();
      return ok(status());
    },

    stopEngine: () => {
      state.supervisor = { ...state.supervisor, state: "stopped" };
      publishStatus();
      return Promise.resolve(status());
    },

    getEngineStatus: () => Promise.resolve(status()),

    setParams: (patch) => {
      const params: TuningParams = { ...state.config.params };
      if (patch.key != null) params.key = patch.key;
      if (patch.scale != null) params.scale = patch.scale;
      if (patch.customMask != null) params.customMask = patch.customMask;
      if (patch.retuneMs != null) params.retuneMs = patch.retuneMs;
      if (patch.humanize != null) params.humanize = patch.humanize;
      if (patch.voiceRange != null) params.voiceRange = patch.voiceRange;
      if (patch.mix != null) params.mix = patch.mix;
      if (patch.gateThresholdDb != null) params.gateThresholdDb = patch.gateThresholdDb;
      if (patch.bypass != null) params.bypass = patch.bypass;
      const rangeChanged = params.voiceRange !== state.config.params.voiceRange;
      state.config = { ...state.config, params };
      if (rangeChanged) publishStatus();
      return Promise.resolve({ ...params });
    },

    setBypass: (bypass) => {
      state.config = { ...state.config, params: { ...state.config.params, bypass } };
      void bypassEvent.emit(bypass);
      return Promise.resolve(bypass);
    },

    runLatencyTest: async () => {
      if (state.supervisor.state !== "running") {
        return fail("notRunning", "Start the tuner before measuring latency.");
      }
      await delay(1800);
      const estimate = engineStatus();
      const dsp = estimate?.dspLatencyMs ?? 10;
      const hardwareMs = (estimate?.estimatedLatencyMs ?? 20) - dsp + 3.4 + Math.random() * 1.5;
      const result = { hardwareMs, totalMs: hardwareMs + dsp, confidence: 0.86 };
      state.config = { ...state.config, measuredLatencyMs: result.totalMs };
      return ok(result);
    },

    runSetupCheck: async () => {
      await delay(120);
      state.setupChecks++;
      return ok(setupReport());
    },

    setRouteAllApps: async (enabled) => {
      if (!setupReport().vbCableInstalled) {
        return fail("device", "VB-Cable is not installed.");
      }
      await delay(100);
      state.config = {
        ...state.config,
        routeAllApps: enabled,
        routingBackup: enabled
          ? { console: DEVICES[0]?.id ?? null, communications: DEVICES[0]?.id ?? null, dirty: true }
          : null,
      };
      return ok(enabled);
    },

    openAsioPanel: () => {
      const mic = selectedMic();
      return Promise.resolve(
        mic?.asioDriver == null
          ? fail("unsupported", "The selected microphone has no ASIO driver.")
          : ok(null),
      );
    },

    getDiagnostics: () => {
      const engine = engineStatus();
      const meters = state.supervisor.state === "running" ? meterSnapshot() : null;
      const lines = [
        `TunedUp ${APP_VERSION} (browser mock)`,
        `OS: ${typeof navigator === "undefined" ? "unknown" : navigator.userAgent}`,
        `Supervisor: ${state.supervisor.state} (restarts ${state.supervisor.restarts})`,
        engine
          ? `Capture: ${engine.capture.deviceName} via ${tierLabel(engine.capture.tier)}, ${engine.capture.sampleRate} Hz, ${engine.capture.periodFrames} frames`
          : "Engine: not running",
        engine ? `Monitor: ${engine.monitor.deviceName} via ${tierLabel(engine.monitor.tier)}` : "",
        engine ? `Estimated latency: ${engine.estimatedLatencyMs.toFixed(1)} ms` : "",
        `Measured latency: ${state.config.measuredLatencyMs?.toFixed(1) ?? "n/a"} ms`,
      ].filter((l) => l !== "");
      const diagnostics: Diagnostics = {
        appVersion: APP_VERSION,
        os: "Windows 11 (mock)",
        audioBackend: "mock",
        supervisor: { ...state.supervisor },
        engine,
        meters,
        measuredLatencyMs: state.config.measuredLatencyMs,
        logDir: "C:\\Users\\you\\AppData\\Local\\TunedUp\\logs",
        configPath: "C:\\Users\\you\\AppData\\Roaming\\TunedUp\\config.json",
        report: lines.join("\n"),
      };
      return Promise.resolve(diagnostics);
    },

    completeWizard: () => {
      state.config = { ...state.config, wizardCompleted: true };
      return Promise.resolve(structuredClone(state.config));
    },

    setLaunchAtLogin: (enabled) => {
      state.config = { ...state.config, launchAtLogin: enabled };
      return Promise.resolve(ok(enabled));
    },

    savePreset: (name) => {
      const trimmed = name.trim();
      if (trimmed === "") return Promise.resolve(fail("config", "Preset name can't be empty."));
      const preset: Preset = { name: trimmed, params: { ...state.config.params } };
      const others = state.config.presets.filter((p) => p.name !== trimmed);
      state.config = { ...state.config, presets: [...others, preset] };
      return Promise.resolve(ok(presetsCopy()));
    },

    loadPreset: (name) => {
      const preset = state.config.presets.find((p) => p.name === name);
      if (!preset) return Promise.resolve(fail("config", `No preset named "${name}".`));
      // Loading keeps the live bypass state.
      const params = { ...preset.params, bypass: state.config.params.bypass };
      const rangeChanged = params.voiceRange !== state.config.params.voiceRange;
      state.config = { ...state.config, params };
      if (rangeChanged) publishStatus();
      return Promise.resolve(ok({ ...params }));
    },

    deletePreset: (name) => {
      state.config = {
        ...state.config,
        presets: state.config.presets.filter((p) => p.name !== name),
      };
      return Promise.resolve(presetsCopy());
    },

    getUpdateStatus: () => Promise.resolve(state.update),

    checkForUpdate: () => {
      setUpdate({ state: "checking" });
      after(900, () => {
        let percent = 0;
        const step = () => {
          setUpdate({ state: "downloading", version: "0.2.0", percent });
          if (percent < 100) {
            percent = Math.min(100, percent + 20);
            after(250, step);
          } else {
            setUpdate({
              state: "ready",
              version: "0.2.0",
              notes: "Smoother retune curve and faster device switching.",
            });
          }
        };
        step();
      });
      return Promise.resolve(ok(state.update));
    },

    installUpdate: () => {
      if (state.update.state !== "ready") {
        return Promise.resolve(fail("update", "No update has been downloaded yet."));
      }
      // The real app restarts here; the mock just pretends it already did.
      setUpdate({ state: "upToDate", current: state.update.version });
      return Promise.resolve(ok(null));
    },

    hideWindow: () => Promise.resolve(),
    quitApp: () => Promise.resolve(),
  };

  return {
    commands,
    events,
    reset: () => {
      for (const id of timers) clearTimeout(id);
      timers.clear();
      state = initialState();
      if (meterTimer !== null) clearInterval(meterTimer);
      meterTimer = null;
      syncMeterLoop();
    },
    setDevices: (devices) => {
      state.devices = devices.map((d) => ({ ...d }));
      void devicesChangedEvent.emit(state.devices.map((d) => ({ ...d })));
    },
  };
}
