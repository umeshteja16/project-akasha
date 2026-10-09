import { MonitorIcon, MoonIcon, SunIcon } from "lucide-react";
import { Card, CardContent } from "@/components/ui/card";
import { Segmented } from "@/components/ui/segmented";
import { type ThemePreference, useTheme } from "@/lib/theme";
import { SettingsSection } from "./settings-section";

const OPTIONS = [
  { value: "system", label: "System", icon: <MonitorIcon /> },
  { value: "light", label: "Light", icon: <SunIcon /> },
  { value: "dark", label: "Dark", icon: <MoonIcon /> },
] as const satisfies ReadonlyArray<{ value: ThemePreference; label: string; icon: unknown }>;

export function AppearanceSection() {
  const { preference, resolved, setPreference } = useTheme();
  return (
    <SettingsSection
      id="appearance"
      title="Appearance"
      description="Paper by day, ink by night. Saved on this device."
    >
      <Card>
        <CardContent className="flex flex-wrap items-center justify-between gap-4">
          <div className="grid gap-0.5">
            <p className="text-sm font-medium text-fg">Theme</p>
            <p className="text-xs text-fg-subtle">
              {preference === "system"
                ? `Following your system (${resolved} now)`
                : `Always ${resolved}`}
            </p>
          </div>
          <Segmented
            label="Theme"
            value={preference}
            onValueChange={setPreference}
            options={OPTIONS}
          />
        </CardContent>
      </Card>
    </SettingsSection>
  );
}
