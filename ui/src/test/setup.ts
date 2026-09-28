import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";

import { mockBackend } from "@/lib/api";
import { meterStore } from "@/lib/meters";
import { clearToasts } from "@/lib/toast";

// --- jsdom gaps ------------------------------------------------------------

// jsdom has no canvas; the meter only needs a context whose calls are no-ops.
const fakeContext = new Proxy(
  {},
  {
    get: (_target, prop) => (prop === "measureText" ? () => ({ width: 10 }) : () => undefined),
    set: () => true,
  },
);
Object.defineProperty(HTMLCanvasElement.prototype, "getContext", {
  configurable: true,
  value: () => fakeContext,
});

class ResizeObserverStub {
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
}
if (!("ResizeObserver" in globalThis)) {
  Object.defineProperty(globalThis, "ResizeObserver", { value: ResizeObserverStub });
}

if (typeof window.matchMedia !== "function") {
  Object.defineProperty(window, "matchMedia", {
    configurable: true,
    value: (query: string) => ({
      matches: false,
      media: query,
      onchange: null,
      addEventListener: () => {},
      removeEventListener: () => {},
      addListener: () => {},
      removeListener: () => {},
      dispatchEvent: () => false,
    }),
  });
}

// Radix Select relies on pointer capture and scrollIntoView.
const proto = Element.prototype;
if (typeof proto.hasPointerCapture !== "function") proto.hasPointerCapture = () => false;
if (typeof proto.setPointerCapture !== "function") proto.setPointerCapture = () => {};
if (typeof proto.releasePointerCapture !== "function") proto.releasePointerCapture = () => {};
if (typeof proto.scrollIntoView !== "function") proto.scrollIntoView = () => {};

afterEach(() => {
  cleanup();
  mockBackend?.reset();
  meterStore.clear();
  clearToasts();
});
