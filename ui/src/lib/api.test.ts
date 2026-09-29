import { describe, expect, it, vi } from "vitest";

import { commands as realCommands, events as realEvents } from "@/bindings";

import { commands, events, isTauri, mockBackend } from "./api";

describe("api mock parity", () => {
  it("uses the mock outside Tauri", () => {
    expect(isTauri).toBe(false);
    expect(mockBackend).not.toBeNull();
    expect(commands).not.toBe(realCommands);
  });

  it("implements every command from bindings.ts", () => {
    for (const key of Object.keys(realCommands)) {
      expect(commands, `missing command ${key}`).toHaveProperty(key);
      expect(typeof Reflect.get(commands, key)).toBe("function");
    }
    expect(Object.keys(commands).toSorted()).toEqual(Object.keys(realCommands).toSorted());
  });

  it("implements every event from bindings.ts with listen/once/emit", () => {
    for (const key of Object.keys(realEvents)) {
      const event: unknown = Reflect.get(events, key);
      expect(event, `missing event ${key}`).toBeTypeOf("function");
      for (const method of ["listen", "once", "emit"]) {
        expect(typeof Reflect.get(Object(event), method)).toBe("function");
      }
    }
  });

  it("returns typedError-shaped results", async () => {
    const devices = await commands.listDevices();
    expect(devices).toMatchObject({
      status: "ok",
      data: expect.arrayContaining([
        expect.objectContaining({ direction: "capture", isDefault: true }),
        expect.objectContaining({ direction: "render", isVbCable: true }),
      ]),
    });
    const bad = await commands.setConfig({ bypassHotkey: "B" });
    expect(bad).toMatchObject({ status: "error", error: { kind: "config" } });
  });

  it("reports a running engine on WASAPI exclusive", async () => {
    const status = await commands.getEngineStatus();
    expect(status.supervisor.state).toBe("running");
    expect(status.engine?.capture.tier).toBe("wasapiExclusive");
  });

  it("animates meters at ~30 Hz while someone listens (FR-17)", async () => {
    vi.useFakeTimers();
    try {
      const received: number[] = [];
      const unlisten = await events.metersEvent.listen((e) => received.push(e.payload.dsp.inputDb));
      vi.advanceTimersByTime(340);
      expect(received.length).toBeGreaterThanOrEqual(9);
      unlisten();
      const count = received.length;
      vi.advanceTimersByTime(340);
      expect(received.length).toBe(count);
    } finally {
      vi.useRealTimers();
    }
  });

  it("emits bypassEvent when bypass is set", async () => {
    const seen: boolean[] = [];
    const unlisten = await events.bypassEvent.listen((e) => seen.push(e.payload));
    await commands.setBypass(true);
    expect(seen).toEqual([true]);
    unlisten();
  });
});
