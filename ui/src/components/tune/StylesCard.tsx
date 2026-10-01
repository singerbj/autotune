import { WandSparklesIcon } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Card, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { useApplyStyle, useStyles } from "@/lib/queries";
import { toast } from "@/lib/toast";

/** FR-27: built-in sounds, applied over your key, range and gate. */
export function StylesCard() {
  const styles = useStyles();
  const apply = useApplyStyle();

  return (
    <Card>
      <CardHeader>
        <CardTitle>Styles</CardTitle>
        <CardDescription>
          Finished pop vocal sounds in one click. Your key, voice range and gate stay as they are.
        </CardDescription>
      </CardHeader>
      <ul className="grid grid-cols-2 gap-2" aria-label="Styles">
        {(styles.data ?? []).map((s) => (
          <li key={s.style}>
            <Button
              variant="outline"
              className="h-auto w-full flex-col items-start gap-1 px-3 py-2 text-left whitespace-normal"
              aria-label={`Apply style ${s.name}`}
              disabled={apply.isPending}
              onClick={() =>
                apply.mutate(s.style, {
                  onSuccess: () => toast({ title: `Applied “${s.name}”`, variant: "success" }),
                })
              }
            >
              <span className="flex items-center gap-1.5 text-sm font-medium">
                <WandSparklesIcon aria-hidden className="size-3.5" />
                {s.name}
              </span>
              <span className="text-xs font-normal text-muted-foreground">{s.description}</span>
            </Button>
          </li>
        ))}
      </ul>
    </Card>
  );
}
