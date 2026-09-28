import { describe, expect, it } from "vitest";

import { AppErrorException, errorMessage, isAppError, unwrap } from "./result";

describe("command results", () => {
  it("unwraps ok results", async () => {
    await expect(unwrap(Promise.resolve({ status: "ok" as const, data: 42 }))).resolves.toBe(42);
  });

  it("throws the AppError message for error results", async () => {
    const pending = unwrap(
      Promise.resolve({
        status: "error" as const,
        error: { kind: "device" as const, message: "Microphone unplugged" },
      }),
    );
    await expect(pending).rejects.toBeInstanceOf(AppErrorException);
    await expect(pending).rejects.toThrow("Microphone unplugged");
  });

  it("extracts messages from anything", () => {
    expect(errorMessage(new Error("boom"))).toBe("boom");
    expect(errorMessage("plain")).toBe("plain");
    expect(errorMessage({ kind: "config", message: "bad" })).toBe("bad");
    expect(errorMessage(null)).toBe("Something went wrong.");
    expect(isAppError({ kind: "nope", message: "x" })).toBe(false);
  });
});
