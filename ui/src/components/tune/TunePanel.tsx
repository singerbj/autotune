import type { ParamsPatch, Scale, TuningParams, VoiceRange } from "@/bindings";
import { DebouncedSlider } from "@/components/DebouncedSlider";
import { SettingRow } from "@/components/SettingRow";
import { Card, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { formatGate, formatPercent, formatSemitones, VOICE_RANGES } from "@/lib/format";
import { NOTE_NAMES } from "@/lib/music";
import { useConfig, useSetParams } from "@/lib/queries";

import { PitchMeter } from "./PitchMeter";
import { PresetsCard } from "./PresetsCard";
import { ScaleKeyboard } from "./ScaleKeyboard";
import { StylesCard } from "./StylesCard";

export const SCALES: ReadonlyArray<{ value: Scale; label: string }> = [
  { value: "chromatic", label: "Chromatic" },
  { value: "major", label: "Major" },
  { value: "naturalMinor", label: "Natural minor" },
  { value: "custom", label: "Custom" },
];

function isScale(value: string): value is Scale {
  return SCALES.some((s) => s.value === value);
}

function isVoiceRange(value: string): value is VoiceRange {
  return VOICE_RANGES.some((r) => r.value === value);
}

/** Key, scale and custom notes (FR-06). */
function KeyScaleCard({
  params,
  send,
}: {
  params: TuningParams;
  send: (patch: ParamsPatch) => void;
}) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>Key &amp; scale</CardTitle>
        <CardDescription>The notes your voice is pulled towards.</CardDescription>
      </CardHeader>
      <div className="grid grid-cols-2 gap-4">
        <div className="flex flex-col gap-2">
          <Label htmlFor="tune-key">Key</Label>
          <Select value={String(params.key)} onValueChange={(v) => send({ key: Number(v) })}>
            <SelectTrigger id="tune-key">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {NOTE_NAMES.map((name, pc) => (
                <SelectItem key={name} value={String(pc)}>
                  {name}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
        <div className="flex flex-col gap-2">
          <Label htmlFor="tune-scale">Scale</Label>
          <Select
            value={params.scale}
            onValueChange={(v) => {
              if (isScale(v)) send({ scale: v });
            }}
          >
            <SelectTrigger id="tune-scale">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {SCALES.map((s) => (
                <SelectItem key={s.value} value={s.value}>
                  {s.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
      </div>
      {params.scale === "custom" && (
        <ScaleKeyboard mask={params.customMask} onChange={(customMask) => send({ customMask })} />
      )}
    </Card>
  );
}

/** Hard tune, retune, humanize, formant, range, mix and gate (FR-07 – FR-10, FR-23, FR-24). */
function SoundCard({ params, send }: { params: TuningParams; send: (patch: ParamsPatch) => void }) {
  const range = VOICE_RANGES.find((r) => r.value === params.voiceRange);
  return (
    <Card>
      <CardHeader>
        <CardTitle>Sound</CardTitle>
        <CardDescription>Changes apply instantly while you sing.</CardDescription>
      </CardHeader>
      <SettingRow
        htmlFor="hard-tune"
        label="Hard tune"
        description="Snaps instantly from note to note for the classic robotic pop sound."
      >
        <Switch
          id="hard-tune"
          checked={params.hardTune}
          onCheckedChange={(hardTune) => send({ hardTune })}
        />
      </SettingRow>
      {params.hardTune && params.scale === "chromatic" && (
        <p className="text-xs text-muted-foreground">
          Tip: choose your song&apos;s key and a major or minor scale. The jumps between scale notes
          are what makes the effect stand out.
        </p>
      )}
      <DebouncedSlider
        label="Retune speed"
        value={params.hardTune ? 0 : params.retuneMs}
        min={0}
        max={200}
        onCommit={(retuneMs) => send({ retuneMs })}
        format={(v) => `${Math.round(v)} ms`}
        minLabel="Robotic"
        maxLabel="Natural"
        disabled={params.hardTune}
        description={params.hardTune ? "Hard tune always snaps at 0 ms." : undefined}
      />
      <DebouncedSlider
        label="Humanize"
        value={params.hardTune ? 0 : Math.round(params.humanize * 100)}
        min={0}
        max={100}
        onCommit={(v) => send({ humanize: v / 100 })}
        format={(v) => `${Math.round(v)}%`}
        disabled={params.hardTune}
        description="Keeps natural pitch movement such as scoops and vibrato."
      />
      <DebouncedSlider
        label="Formant"
        value={params.formantSemitones}
        min={-4}
        max={4}
        step={0.5}
        onCommit={(formantSemitones) => send({ formantSemitones })}
        format={formatSemitones}
        minLabel="Deeper"
        maxLabel="Brighter"
        description="Changes the size of the voice without changing the note."
      />
      <div className="flex flex-col gap-2">
        <span id="voice-range-label" className="text-sm font-medium">
          Voice range
        </span>
        <ToggleGroup
          type="single"
          aria-labelledby="voice-range-label"
          value={params.voiceRange}
          onValueChange={(v) => {
            // Radix allows deselecting; a range must always be chosen.
            if (isVoiceRange(v) && v !== params.voiceRange) send({ voiceRange: v });
          }}
        >
          {VOICE_RANGES.map((r) => (
            <ToggleGroupItem key={r.value} value={r.value} aria-label={`${r.label} voice range`}>
              <span>{r.label}</span>
              <span className="text-[10px] font-normal opacity-70">from {r.lowestHz} Hz</span>
            </ToggleGroupItem>
          ))}
        </ToggleGroup>
        {range && (
          <p className="text-xs text-muted-foreground">
            Tracks down to {range.lowestHz} Hz · adds ≈{range.latencyMs} ms of processing latency.
          </p>
        )}
      </div>
      <DebouncedSlider
        label="Dry / wet mix"
        value={Math.round(params.mix * 100)}
        min={0}
        max={100}
        onCommit={(v) => send({ mix: v / 100 })}
        format={(v) => formatPercent(v / 100)}
        minLabel="Dry"
        maxLabel="Tuned"
      />
      <DebouncedSlider
        label="Noise gate"
        value={params.gateThresholdDb}
        min={-100}
        max={0}
        onCommit={(gateThresholdDb) => send({ gateThresholdDb })}
        format={formatGate}
        minLabel="Off"
        maxLabel="0 dBFS"
        description="Silences the output below this level (keyboard clicks, fans)."
      />
    </Card>
  );
}

/** Tune tab (FR-06 – FR-11, FR-17, FR-21, FR-23, FR-24, FR-27). */
export function TunePanel() {
  const config = useConfig();
  const setParams = useSetParams();

  if (!config.data) {
    return <p className="p-6 text-sm text-muted-foreground">Loading…</p>;
  }
  const { params, presets } = config.data;
  // FR-11: every control sends a minimal patch; the reply updates the cache.
  const send = (patch: ParamsPatch) => setParams.mutate(patch);

  return (
    <div className="grid grid-cols-[minmax(0,1.15fr)_minmax(0,1fr)] gap-4">
      <div className="flex min-w-0 flex-col gap-4">
        <PitchMeter gateThresholdDb={params.gateThresholdDb} bypass={params.bypass} />
        <KeyScaleCard params={params} send={send} />
        <PresetsCard presets={presets} />
      </div>
      <div className="flex min-w-0 flex-col gap-4">
        <StylesCard />
        <SoundCard params={params} send={send} />
      </div>
    </div>
  );
}
