import type { DelayDivision, FxPatch, FxParams, FxRoute } from "@/bindings";
import { DebouncedSlider } from "@/components/DebouncedSlider";
import { Card, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { DELAY_DIVISIONS, formatDb, formatPercent, FX_ROUTES } from "@/lib/format";
import { useConfig, useSetParams } from "@/lib/queries";

type Send = (patch: FxPatch) => void;

function isRoute(value: string): value is FxRoute {
  return FX_ROUTES.some((r) => r.value === value);
}

function isDivision(value: string): value is DelayDivision {
  return DELAY_DIVISIONS.some((d) => d.value === value);
}

/** FR-26: where a time-based effect is heard. */
function RouteToggle({
  effect,
  value,
  onChange,
}: {
  effect: string;
  value: FxRoute;
  onChange: (route: FxRoute) => void;
}) {
  const labelId = `${effect.toLowerCase()}-route-label`;
  return (
    <div className="flex flex-col gap-2">
      <span id={labelId} className="text-sm font-medium">
        Heard in
      </span>
      <ToggleGroup
        type="single"
        aria-labelledby={labelId}
        value={value}
        onValueChange={(v) => {
          // Radix allows deselecting; a route must always be chosen.
          if (isRoute(v) && v !== value) onChange(v);
        }}
      >
        {FX_ROUTES.map((r) => (
          <ToggleGroupItem key={r.value} value={r.value} aria-label={`${effect} ${r.long}`}>
            {r.label}
          </ToggleGroupItem>
        ))}
      </ToggleGroup>
    </div>
  );
}

const percent = (v: number) => formatPercent(v / 100);

/** Presence, air and compressor: heard everywhere (FR-25). */
function VoiceCard({ fx, send }: { fx: FxParams; send: Send }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>Voice</CardTitle>
        <CardDescription>
          Tone and loudness, like a vocal on the radio. Heard in your headphones and in Discord.
        </CardDescription>
      </CardHeader>
      <DebouncedSlider
        label="Presence"
        value={fx.presenceDb}
        min={-6}
        max={12}
        step={0.5}
        onCommit={(presenceDb) => send({ presenceDb })}
        format={formatDb}
        description="Pushes the voice forward (around 3.5 kHz)."
      />
      <DebouncedSlider
        label="Air"
        value={fx.airDb}
        min={-6}
        max={12}
        step={0.5}
        onCommit={(airDb) => send({ airDb })}
        format={formatDb}
        description="Adds sparkle and breath (above 10 kHz)."
      />
      <DebouncedSlider
        label="Compressor threshold"
        value={fx.compThresholdDb}
        min={-40}
        max={0}
        onCommit={(compThresholdDb) => send({ compThresholdDb })}
        format={(v) => (v >= 0 ? "Off" : `${Math.round(v)} dBFS`)}
        minLabel="Heavy"
        maxLabel="Off"
        description="Evens out loud and quiet words; lower means more squeeze."
      />
      <DebouncedSlider
        label="Compressor ratio"
        value={fx.compRatio}
        min={1}
        max={10}
        step={0.5}
        onCommit={(compRatio) => send({ compRatio })}
        format={(v) => `${v}:1`}
        disabled={fx.compThresholdDb >= 0}
      />
    </Card>
  );
}

/** Two detuned, delayed copies of the voice (FR-25). */
function DoublerCard({ fx, send }: { fx: FxParams; send: Send }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>Doubler</CardTitle>
        <CardDescription>Stacks copies of your voice for a thick, layered sound.</CardDescription>
      </CardHeader>
      <DebouncedSlider
        label="Doubler level"
        value={Math.round(fx.doublerMix * 100)}
        min={0}
        max={100}
        onCommit={(v) => send({ doublerMix: v / 100 })}
        format={(v) => (v === 0 ? "Off" : percent(v))}
      />
      <DebouncedSlider
        label="Detune"
        value={fx.doublerDetuneCents}
        min={0}
        max={30}
        onCommit={(doublerDetuneCents) => send({ doublerDetuneCents })}
        format={(v) => `${Math.round(v)} cents`}
        minLabel="Tight"
        maxLabel="Wide"
      />
      <DebouncedSlider
        label="Doubler delay"
        value={fx.doublerDelayMs}
        min={10}
        max={40}
        onCommit={(doublerDelayMs) => send({ doublerDelayMs })}
        format={(v) => `${Math.round(v)} ms`}
      />
      <RouteToggle
        effect="Doubler"
        value={fx.doublerRoute}
        onChange={(doublerRoute) => send({ doublerRoute })}
      />
    </Card>
  );
}

/** Tempo-synced echo (FR-25). */
function EchoCard({ fx, send }: { fx: FxParams; send: Send }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>Echo</CardTitle>
        <CardDescription>Repeats in time with your song. Set the tempo to match.</CardDescription>
      </CardHeader>
      <DebouncedSlider
        label="Echo level"
        value={Math.round(fx.delayMix * 100)}
        min={0}
        max={100}
        onCommit={(v) => send({ delayMix: v / 100 })}
        format={(v) => (v === 0 ? "Off" : percent(v))}
      />
      <div className="grid grid-cols-2 gap-4">
        <DebouncedSlider
          label="Tempo"
          value={fx.delayBpm}
          min={60}
          max={200}
          onCommit={(delayBpm) => send({ delayBpm })}
          format={(v) => `${Math.round(v)} BPM`}
        />
        <div className="flex flex-col gap-2">
          <Label htmlFor="echo-division">Note</Label>
          <Select
            value={fx.delayDivision}
            onValueChange={(v) => {
              if (isDivision(v)) send({ delayDivision: v });
            }}
          >
            <SelectTrigger id="echo-division">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {DELAY_DIVISIONS.map((d) => (
                <SelectItem key={d.value} value={d.value}>
                  {d.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
      </div>
      <DebouncedSlider
        label="Feedback"
        value={Math.round(fx.delayFeedback * 100)}
        min={0}
        max={90}
        onCommit={(v) => send({ delayFeedback: v / 100 })}
        format={percent}
        minLabel="One repeat"
        maxLabel="Many"
      />
      <RouteToggle
        effect="Echo"
        value={fx.delayRoute}
        onChange={(delayRoute) => send({ delayRoute })}
      />
    </Card>
  );
}

/** Plate reverb (FR-25). */
function ReverbCard({ fx, send }: { fx: FxParams; send: Send }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>Reverb</CardTitle>
        <CardDescription>A plate reverb: the polished space around a record vocal.</CardDescription>
      </CardHeader>
      <DebouncedSlider
        label="Reverb level"
        value={Math.round(fx.reverbMix * 100)}
        min={0}
        max={100}
        onCommit={(v) => send({ reverbMix: v / 100 })}
        format={(v) => (v === 0 ? "Off" : percent(v))}
      />
      <DebouncedSlider
        label="Size"
        value={Math.round(fx.reverbSize * 100)}
        min={0}
        max={100}
        onCommit={(v) => send({ reverbSize: v / 100 })}
        format={percent}
        minLabel="Small"
        maxLabel="Large"
      />
      <DebouncedSlider
        label="Decay"
        value={Math.round(fx.reverbDecay * 100)}
        min={0}
        max={100}
        onCommit={(v) => send({ reverbDecay: v / 100 })}
        format={percent}
        minLabel="Short"
        maxLabel="Long"
      />
      <DebouncedSlider
        label="Pre-delay"
        value={fx.reverbPredelayMs}
        min={0}
        max={100}
        onCommit={(reverbPredelayMs) => send({ reverbPredelayMs })}
        format={(v) => `${Math.round(v)} ms`}
        description="A short gap keeps the words clear before the reverb blooms."
      />
      <RouteToggle
        effect="Reverb"
        value={fx.reverbRoute}
        onChange={(reverbRoute) => send({ reverbRoute })}
      />
    </Card>
  );
}

/** Effects tab (FR-25, FR-26). None of these add latency. */
export function EffectsPanel() {
  const config = useConfig();
  const setParams = useSetParams();

  if (!config.data) {
    return <p className="p-6 text-sm text-muted-foreground">Loading…</p>;
  }
  const { fx } = config.data.params;
  // Every control sends only the field it changed.
  const send: Send = (patch) => setParams.mutate({ fx: patch });

  return (
    <div className="grid grid-cols-2 gap-4">
      <div className="flex min-w-0 flex-col gap-4">
        <VoiceCard fx={fx} send={send} />
        <DoublerCard fx={fx} send={send} />
      </div>
      <div className="flex min-w-0 flex-col gap-4">
        <EchoCard fx={fx} send={send} />
        <ReverbCard fx={fx} send={send} />
      </div>
    </div>
  );
}
