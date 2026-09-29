import type { ReactNode } from "react";

import { Label } from "@/components/ui/label";

/** Label + help text on the left, a control on the right. */
export function SettingRow({
  htmlFor,
  label,
  description,
  children,
}: {
  htmlFor: string;
  label: string;
  description?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div className="flex items-center justify-between gap-6">
      <div className="flex min-w-0 flex-col gap-1">
        <Label htmlFor={htmlFor}>{label}</Label>
        {description !== undefined && (
          <p className="text-xs text-muted-foreground">{description}</p>
        )}
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  );
}
