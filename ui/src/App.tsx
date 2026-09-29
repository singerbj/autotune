import { ActivityIcon, AudioLinesIcon, SettingsIcon, SlidersVerticalIcon } from "lucide-react";
import { useState } from "react";

import { AudioPanel } from "@/components/audio/AudioPanel";
import { DiagnosticsPanel } from "@/components/diagnostics/DiagnosticsPanel";
import { Header } from "@/components/Header";
import { SettingsPanel } from "@/components/settings/SettingsPanel";
import { TunePanel } from "@/components/tune/TunePanel";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Toaster } from "@/components/ui/toaster";
import { TooltipProvider } from "@/components/ui/tooltip";
import { SetupWizard } from "@/components/wizard/SetupWizard";
import { useAppInfo } from "@/lib/queries";
import { useBackendEvents } from "@/lib/useBackendEvents";

export function App() {
  useBackendEvents();
  const appInfo = useAppInfo();
  // `null` = follow the backend's showWizard; a boolean = the user opened/closed it.
  const [wizardOpen, setWizardOpen] = useState<boolean | null>(null);
  const showWizard = wizardOpen ?? appInfo.data?.showWizard ?? false;

  return (
    <TooltipProvider delayDuration={300}>
      <div className="flex h-full flex-col">
        <Header />
        <Tabs defaultValue="tune" className="flex min-h-0 flex-1 flex-col">
          <div className="border-b px-5 py-2">
            <TabsList aria-label="Sections">
              <TabsTrigger value="tune">
                <SlidersVerticalIcon aria-hidden />
                Tune
              </TabsTrigger>
              <TabsTrigger value="audio">
                <AudioLinesIcon aria-hidden />
                Audio
              </TabsTrigger>
              <TabsTrigger value="diagnostics">
                <ActivityIcon aria-hidden />
                Diagnostics
              </TabsTrigger>
              <TabsTrigger value="settings">
                <SettingsIcon aria-hidden />
                Settings
              </TabsTrigger>
            </TabsList>
          </div>
          <main className="min-h-0 flex-1 overflow-y-auto p-5">
            <div className="mx-auto max-w-6xl">
              <TabsContent value="tune">
                <TunePanel />
              </TabsContent>
              <TabsContent value="audio">
                <AudioPanel />
              </TabsContent>
              <TabsContent value="diagnostics">
                <DiagnosticsPanel />
              </TabsContent>
              <TabsContent value="settings">
                <SettingsPanel onOpenWizard={() => setWizardOpen(true)} />
              </TabsContent>
            </div>
          </main>
        </Tabs>
      </div>
      <SetupWizard open={showWizard} onOpenChange={setWizardOpen} />
      <Toaster />
    </TooltipProvider>
  );
}
