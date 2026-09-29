import { useEffect, useRef } from "react";

import { Card, CardHeader, CardTitle } from "@/components/ui/card";
import { meterStore, useMeterSnapshot } from "@/lib/meters";
import { hzToNoteName, midiToNoteName } from "@/lib/music";

import { drawMeter, readMeterColors, type MeterColors } from "./drawMeter";

/** Snapshots older than this are treated as "no signal". */
const STALE_MS = 500;
const HEIGHT = 190;

/**
 * FR-17: live meters on a canvas, redrawn every animation frame from the
 * ref-based meter store (no React state at meter rate).
 */
export function PitchMeter({
  gateThresholdDb,
  bypass,
}: {
  gateThresholdDb: number;
  bypass: boolean;
}) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const propsRef = useRef({ gateThresholdDb, bypass });

  useEffect(() => {
    propsRef.current = { gateThresholdDb, bypass };
  }, [gateThresholdDb, bypass]);

  useEffect(() => {
    const canvas = canvasRef.current;
    const ctx = canvas?.getContext("2d") ?? null;
    if (!canvas || !ctx) return;

    let colors: MeterColors = readMeterColors(canvas);
    const media = window.matchMedia?.("(prefers-color-scheme: light)");
    const onScheme = () => {
      colors = readMeterColors(canvas);
    };
    media?.addEventListener("change", onScheme);

    let needle = 0;
    let frame = 0;
    const render = () => {
      frame = requestAnimationFrame(render);
      const dpr = window.devicePixelRatio || 1;
      const cssW = canvas.clientWidth;
      const cssH = canvas.clientHeight;
      const w = Math.round(cssW * dpr);
      const h = Math.round(cssH * dpr);
      if (w === 0 || h === 0) return;
      if (canvas.width !== w || canvas.height !== h) {
        canvas.width = w;
        canvas.height = h;
      }
      const fresh = performance.now() - meterStore.receivedAt < STALE_MS;
      const snapshot = fresh ? meterStore.latest : null;
      const targetCents = snapshot?.dsp.voiced ? snapshot.dsp.correctionCents : 0;
      needle += (targetCents - needle) * 0.35;
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      drawMeter(
        ctx,
        cssW,
        cssH,
        {
          snapshot,
          needleCents: needle,
          gateThresholdDb: propsRef.current.gateThresholdDb,
          bypass: propsRef.current.bypass,
        },
        colors,
      );
    };
    frame = requestAnimationFrame(render);
    return () => {
      cancelAnimationFrame(frame);
      media?.removeEventListener("change", onScheme);
    };
  }, []);

  return (
    <Card className="gap-2 p-0">
      <CardHeader className="px-5 pt-4">
        <CardTitle>Live pitch</CardTitle>
      </CardHeader>
      <canvas
        ref={canvasRef}
        role="img"
        aria-label="Live pitch meter: detected note, target note, correction and input level"
        className="w-full"
        style={{ height: HEIGHT }}
      />
      <MeterReadout />
    </Card>
  );
}

/** Twice-a-second text version of the meter for screen readers. */
function MeterReadout() {
  const snapshot = useMeterSnapshot(500);
  const dsp = snapshot?.dsp;
  const text =
    dsp && dsp.voiced
      ? `Detected ${hzToNoteName(dsp.detectedHz) ?? "none"}, target ${midiToNoteName(dsp.targetMidi) ?? "none"}, correction ${Math.round(dsp.correctionCents)} cents, input ${Math.round(dsp.inputDb)} dBFS`
      : "No pitch detected";
  return <p className="sr-only">{text}</p>;
}
