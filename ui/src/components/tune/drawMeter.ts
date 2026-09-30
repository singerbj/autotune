import type { MeterSnapshot } from "@/bindings";
import {
  centsToFraction,
  clampCents,
  dbToFraction,
  hzToNoteName,
  midiToNoteName,
} from "@/lib/music";

export interface MeterColors {
  foreground: string;
  muted: string;
  mutedForeground: string;
  primary: string;
  success: string;
  warning: string;
  destructive: string;
  border: string;
}

export interface MeterView {
  snapshot: MeterSnapshot | null;
  /** Smoothed needle position in cents. */
  needleCents: number;
  gateThresholdDb: number;
  bypass: boolean;
}

const FALLBACK: MeterColors = {
  foreground: "#eee",
  muted: "#333",
  mutedForeground: "#999",
  primary: "#8b6cff",
  success: "#3ecf8e",
  warning: "#f5b942",
  destructive: "#f0524f",
  border: "#444",
};

/** Resolve theme tokens (they are CSS variables) for canvas drawing. */
export function readMeterColors(el: Element): MeterColors {
  const style = getComputedStyle(el);
  const read = (name: string, fallback: string) => {
    const v = style.getPropertyValue(name).trim();
    return v === "" ? fallback : v;
  };
  return {
    foreground: read("--foreground", FALLBACK.foreground),
    muted: read("--muted", FALLBACK.muted),
    mutedForeground: read("--muted-foreground", FALLBACK.mutedForeground),
    primary: read("--primary", FALLBACK.primary),
    success: read("--success", FALLBACK.success),
    warning: read("--warning", FALLBACK.warning),
    destructive: read("--destructive", FALLBACK.destructive),
    border: read("--border", FALLBACK.border),
  };
}

function roundRect(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  w: number,
  h: number,
  r: number,
) {
  ctx.beginPath();
  ctx.roundRect(x, y, Math.max(0, w), h, r);
  ctx.fill();
}

function pill(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  label: string,
  on: boolean,
  onColor: string,
  c: MeterColors,
): number {
  ctx.font = "600 11px system-ui, sans-serif";
  const w = ctx.measureText(label).width + 26;
  ctx.fillStyle = c.muted;
  roundRect(ctx, x, y, w, 20, 10);
  ctx.fillStyle = on ? onColor : c.border;
  ctx.beginPath();
  ctx.arc(x + 10, y + 10, 4, 0, Math.PI * 2);
  ctx.fill();
  ctx.fillStyle = on ? c.foreground : c.mutedForeground;
  ctx.textBaseline = "middle";
  ctx.textAlign = "left";
  ctx.fillText(label, x + 19, y + 10.5);
  return w;
}

/** Draws the whole meter in CSS pixels (the caller applies the DPR transform). */
export function drawMeter(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  view: MeterView,
  c: MeterColors,
): void {
  ctx.clearRect(0, 0, width, height);
  const pad = 16;
  const dsp = view.snapshot?.dsp ?? null;
  const voiced = dsp !== null && dsp.voiced && dsp.detectedHz > 0;

  // --- note readouts -------------------------------------------------------
  const detected = voiced ? (hzToNoteName(dsp.detectedHz) ?? "—") : "—";
  const target = dsp === null || view.bypass ? null : midiToNoteName(dsp.targetMidi);
  ctx.textBaseline = "alphabetic";
  ctx.textAlign = "left";
  ctx.fillStyle = c.mutedForeground;
  ctx.font = "500 11px system-ui, sans-serif";
  ctx.fillText("DETECTED", pad, pad + 10);
  ctx.fillStyle = voiced ? c.foreground : c.mutedForeground;
  ctx.font = "600 34px system-ui, sans-serif";
  ctx.fillText(detected, pad, pad + 48);
  ctx.fillStyle = c.mutedForeground;
  ctx.font = "400 12px ui-monospace, monospace";
  ctx.fillText(voiced ? `${dsp.detectedHz.toFixed(1)} Hz` : "no pitch", pad, pad + 66);

  const col2 = Math.max(pad + 130, width * 0.3);
  ctx.fillStyle = c.mutedForeground;
  ctx.font = "500 11px system-ui, sans-serif";
  ctx.fillText("TARGET", col2, pad + 10);
  ctx.fillStyle = target !== null && voiced ? c.primary : c.mutedForeground;
  ctx.font = "600 34px system-ui, sans-serif";
  ctx.fillText(view.bypass ? "off" : (target ?? "—"), col2, pad + 48);

  // --- indicators ----------------------------------------------------------
  let px = width - pad;
  const gateLabel = view.gateThresholdDb <= -100 ? "Gate off" : "Gate open";
  const gateOn = dsp?.gateOpen ?? false;
  ctx.font = "600 11px system-ui, sans-serif";
  px -= ctx.measureText(gateLabel).width + 26;
  pill(ctx, px, pad, gateLabel, gateOn, c.success, c);
  px -= ctx.measureText("Voiced").width + 26 + 8;
  pill(ctx, px, pad, "Voiced", voiced, c.primary, c);

  // --- correction needle (−100…+100 cents) --------------------------------
  const barX = pad;
  const barW = width - pad * 2;
  const needleY = pad + 92;
  ctx.fillStyle = c.muted;
  roundRect(ctx, barX, needleY, barW, 10, 5);
  ctx.fillStyle = c.border;
  for (const cents of [-100, -50, 0, 50, 100]) {
    const x = barX + centsToFraction(cents) * barW;
    ctx.fillRect(x - 0.5, needleY - 4, 1, cents === 0 ? 18 : 12);
  }
  const cents = clampCents(view.needleCents);
  const centre = barX + barW / 2;
  const nx = barX + centsToFraction(cents) * barW;
  ctx.fillStyle = Math.abs(cents) > 50 ? c.warning : c.primary;
  roundRect(ctx, Math.min(centre, nx), needleY + 2, Math.abs(nx - centre), 6, 3);
  ctx.fillStyle = c.foreground;
  roundRect(ctx, nx - 2, needleY - 6, 4, 22, 2);
  ctx.fillStyle = c.mutedForeground;
  ctx.font = "400 10px system-ui, sans-serif";
  ctx.textAlign = "left";
  ctx.fillText("−100¢", barX, needleY + 30);
  ctx.textAlign = "center";
  ctx.fillText(
    `correction ${cents >= 0 ? "+" : "−"}${Math.abs(cents).toFixed(0)}¢`,
    centre,
    needleY + 30,
  );
  ctx.textAlign = "right";
  ctx.fillText("+100¢", barX + barW, needleY + 30);

  // --- input level (−60…0 dBFS) -------------------------------------------
  const levelY = height - pad - 24;
  const db = dsp?.inputDb ?? -Infinity;
  const frac = dbToFraction(db);
  ctx.fillStyle = c.muted;
  roundRect(ctx, barX, levelY, barW, 8, 4);
  ctx.fillStyle = db > -6 ? c.destructive : db > -18 ? c.warning : c.success;
  roundRect(ctx, barX, levelY, frac * barW, 8, 4);
  if (view.gateThresholdDb > -100) {
    const gx = barX + dbToFraction(view.gateThresholdDb) * barW;
    ctx.fillStyle = c.foreground;
    ctx.fillRect(gx - 1, levelY - 4, 2, 16);
  }
  ctx.fillStyle = c.mutedForeground;
  ctx.font = "400 10px system-ui, sans-serif";
  ctx.textAlign = "left";
  ctx.fillText("INPUT  −60 dB", barX, levelY + 22);
  ctx.textAlign = "right";
  ctx.fillText(Number.isFinite(db) ? `${db.toFixed(1)} dBFS` : "— dBFS", barX + barW, levelY + 22);
}
