import { ArrowLeftIcon, ArrowRightIcon, CheckIcon } from "lucide-react";
import { useState, type ComponentType } from "react";

import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { useCompleteWizard } from "@/lib/queries";
import { cn } from "@/lib/utils";

import {
  CableStep,
  DevicesStep,
  DiscordStep,
  DoneStep,
  LatencyStep,
  WarningsStep,
  WelcomeStep,
} from "./steps";

export const WIZARD_STEPS: ReadonlyArray<{
  id: string;
  title: string;
  heading: string;
  Component: ComponentType;
}> = [
  { id: "welcome", title: "Welcome", heading: "Welcome to Voice Tuner", Component: WelcomeStep },
  { id: "devices", title: "Devices", heading: "Choose your devices", Component: DevicesStep },
  {
    id: "cable",
    title: "Virtual mic",
    heading: "Virtual microphone (VB-Cable)",
    Component: CableStep,
  },
  { id: "warnings", title: "Checks", heading: "Headset checks", Component: WarningsStep },
  { id: "discord", title: "Discord", heading: "Set up Discord", Component: DiscordStep },
  { id: "latency", title: "Latency", heading: "Measure latency", Component: LatencyStep },
  { id: "done", title: "Done", heading: "Done", Component: DoneStep },
];

/** First-run setup wizard (FR-12, FR-13), shown as a full-screen overlay. */
export function SetupWizard({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const [index, setIndex] = useState(0);
  const complete = useCompleteWizard();
  const last = WIZARD_STEPS.length - 1;
  const step = WIZARD_STEPS[Math.min(index, last)] ?? WIZARD_STEPS[0];
  if (!step) return null;
  const { Component } = step;

  const close = (next: boolean) => {
    if (!next) setIndex(0);
    onOpenChange(next);
  };

  const finish = () => {
    complete.mutate(undefined, { onSuccess: () => close(false) });
  };

  return (
    <Dialog open={open} onOpenChange={close}>
      <DialogContent
        className="top-0 left-0 flex h-full max-w-none translate-x-0 translate-y-0 flex-col gap-0 rounded-none border-0 p-0"
        overlayClassName="bg-background"
      >
        <div className="flex items-center justify-between border-b px-8 py-4 pr-14">
          <span className="text-base font-semibold">Voice Tuner setup</span>
          <ol className="flex items-center gap-1" aria-label="Setup steps">
            {WIZARD_STEPS.map((s, i) => (
              <li
                key={s.id}
                aria-current={i === index ? "step" : undefined}
                className={cn(
                  "flex items-center gap-1.5 rounded-full px-2.5 py-1 text-xs",
                  i === index && "bg-primary/15 font-medium text-primary",
                  i < index && "text-foreground",
                  i > index && "text-muted-foreground",
                )}
              >
                <span
                  aria-hidden
                  className={cn(
                    "flex size-4 items-center justify-center rounded-full border text-[10px]",
                    i < index && "border-success bg-success text-background",
                    i === index && "border-primary",
                  )}
                >
                  {i < index ? <CheckIcon className="size-3" /> : i + 1}
                </span>
                {s.title}
              </li>
            ))}
          </ol>
        </div>

        <div className="flex min-h-0 flex-1 justify-center overflow-y-auto px-8 py-10">
          <section
            className="flex w-full max-w-2xl flex-col gap-6"
            aria-labelledby="wizard-heading"
          >
            <div className="flex flex-col gap-1">
              <p className="text-xs text-muted-foreground">
                Step {index + 1} of {WIZARD_STEPS.length}
              </p>
              <DialogTitle id="wizard-heading" className="text-2xl tracking-tight">
                {step.heading}
              </DialogTitle>
              <DialogDescription className="sr-only">
                First-run setup, step {index + 1} of {WIZARD_STEPS.length}: {step.title}
              </DialogDescription>
            </div>
            <Component key={step.id} />
          </section>
        </div>

        <div className="flex items-center justify-between border-t px-8 py-4">
          <Button
            variant="ghost"
            disabled={index === 0}
            onClick={() => setIndex((i) => Math.max(0, i - 1))}
          >
            <ArrowLeftIcon aria-hidden />
            Back
          </Button>
          {index < last ? (
            <Button onClick={() => setIndex((i) => Math.min(last, i + 1))}>
              Next
              <ArrowRightIcon aria-hidden />
            </Button>
          ) : (
            <Button variant="success" disabled={complete.isPending} onClick={finish}>
              <CheckIcon aria-hidden />
              Finish
            </Button>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}
