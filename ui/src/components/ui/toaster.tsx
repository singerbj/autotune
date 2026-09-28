import { CircleAlertIcon, CircleCheckIcon, InfoIcon, XIcon } from "lucide-react";

import { dismissToast, useToasts, type ToastVariant } from "@/lib/toast";
import { cn } from "@/lib/utils";

const ICONS: Record<ToastVariant, typeof InfoIcon> = {
  default: InfoIcon,
  success: CircleCheckIcon,
  error: CircleAlertIcon,
};

/** Bottom-right stack of toasts; errors are announced assertively. */
export function Toaster() {
  const toasts = useToasts();
  return (
    <div
      aria-live="polite"
      aria-label="Notifications"
      role="region"
      className="pointer-events-none fixed right-4 bottom-4 z-[100] flex w-96 max-w-[calc(100vw-2rem)] flex-col gap-2"
    >
      {toasts.map((t) => {
        const Icon = ICONS[t.variant];
        return (
          <div
            key={t.id}
            role={t.variant === "error" ? "alert" : "status"}
            className={cn(
              "pointer-events-auto flex items-start gap-3 rounded-lg border bg-popover p-3 text-sm text-popover-foreground shadow-lg animate-in fade-in-0 slide-in-from-bottom-2",
              t.variant === "error" && "border-destructive/50",
              t.variant === "success" && "border-success/50",
            )}
          >
            <Icon
              aria-hidden
              className={cn(
                "mt-0.5 size-4 shrink-0 text-primary",
                t.variant === "error" && "text-destructive",
                t.variant === "success" && "text-success",
              )}
            />
            <div className="flex min-w-0 flex-1 flex-col gap-0.5">
              <p className="font-medium">{t.title}</p>
              {t.description !== undefined && (
                <p className="break-words text-muted-foreground">{t.description}</p>
              )}
            </div>
            <button
              type="button"
              aria-label="Dismiss notification"
              onClick={() => dismissToast(t.id)}
              className="rounded-sm opacity-60 outline-none hover:opacity-100 focus-visible:ring-[3px] focus-visible:ring-ring/50"
            >
              <XIcon className="size-4" />
            </button>
          </div>
        );
      })}
    </div>
  );
}
