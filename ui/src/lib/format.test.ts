import { describe, expect, it } from "vitest";

import { formatGate, framesToMs, tierLabel } from "./format";

describe("format helpers", () => {
  it("labels backend tiers (FR-02)", () => {
    expect(tierLabel("asio")).toBe("ASIO");
    expect(tierLabel("wasapiExclusive")).toBe("WASAPI exclusive");
    expect(tierLabel("wasapiSharedLowLatency")).toBe("WASAPI shared (low latency)");
    expect(tierLabel("wasapiShared")).toBe("WASAPI shared");
  });

  it("converts periods to ms (FR-18)", () => {
    expect(framesToMs(480, 48_000)).toBe(10);
    expect(framesToMs(128, 0)).toBe(0);
  });

  it("shows −100 dB gate as Off (FR-09)", () => {
    expect(formatGate(-100)).toBe("Off");
    expect(formatGate(-42.4)).toBe("-42 dBFS");
  });
});
