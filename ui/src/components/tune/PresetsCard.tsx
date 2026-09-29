import { SaveIcon, Trash2Icon, UploadIcon } from "lucide-react";
import { useState, type FormEvent } from "react";

import type { Preset } from "@/bindings";
import { Button } from "@/components/ui/button";
import { Card, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { useDeletePreset, useLoadPreset, useSavePreset } from "@/lib/queries";
import { toast } from "@/lib/toast";

/** FR-21: named presets stored in the config. */
export function PresetsCard({ presets }: { presets: Preset[] }) {
  const [name, setName] = useState("");
  const save = useSavePreset();
  const load = useLoadPreset();
  const remove = useDeletePreset();

  const onSubmit = (e: FormEvent) => {
    e.preventDefault();
    const trimmed = name.trim();
    if (trimmed === "") return;
    save.mutate(trimmed, {
      onSuccess: () => {
        setName("");
        toast({ title: `Saved preset “${trimmed}”`, variant: "success" });
      },
    });
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle>Presets</CardTitle>
        <CardDescription>
          Save the current sound and switch back to it in one click.
        </CardDescription>
      </CardHeader>
      <form onSubmit={onSubmit} className="flex gap-2">
        <Input
          aria-label="Preset name"
          placeholder="Preset name"
          value={name}
          maxLength={64}
          onChange={(e) => setName(e.target.value)}
        />
        <Button type="submit" variant="secondary" disabled={name.trim() === "" || save.isPending}>
          <SaveIcon aria-hidden />
          Save
        </Button>
      </form>
      {presets.length === 0 ? (
        <p className="text-sm text-muted-foreground">No presets yet.</p>
      ) : (
        <ul className="flex flex-col divide-y rounded-md border" aria-label="Saved presets">
          {presets.map((preset) => (
            <li key={preset.name} className="flex items-center gap-2 px-3 py-1.5">
              <span className="min-w-0 flex-1 truncate text-sm">{preset.name}</span>
              <Button
                size="sm"
                variant="ghost"
                aria-label={`Load preset ${preset.name}`}
                disabled={load.isPending}
                onClick={() =>
                  load.mutate(preset.name, {
                    onSuccess: () => toast({ title: `Loaded “${preset.name}”` }),
                  })
                }
              >
                <UploadIcon aria-hidden />
                Load
              </Button>
              <Button
                size="icon"
                variant="ghost"
                className="size-8 text-muted-foreground hover:text-destructive"
                aria-label={`Delete preset ${preset.name}`}
                disabled={remove.isPending}
                onClick={() => remove.mutate(preset.name)}
              >
                <Trash2Icon aria-hidden />
              </Button>
            </li>
          ))}
        </ul>
      )}
    </Card>
  );
}
