import { useQuery } from "@tanstack/react-query";
import { getRouteApi } from "@tanstack/react-router";
import { useApi } from "@/api/context";
import { meQuery, metaQuery } from "@/api/queries";
import { PageHeader } from "@/components/common/page-header";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { useDocumentTitle } from "@/lib/use-document-title";
import { AppearanceSection } from "./appearance-section";
import { DangerSection } from "./danger-section";
import { PasswordSection } from "./password-section";
import { ProfileSection } from "./profile-section";
import { SystemPanel } from "./system-panel";

const route = getRouteApi("/app/settings");

export function SettingsPage() {
  const api = useApi();
  const { data: me } = useQuery(meQuery(api));
  const meta = useQuery(metaQuery(api));
  const { tab = "account" } = route.useSearch();
  useDocumentTitle(tab === "system" ? "System · Settings" : "Settings");
  const navigate = route.useNavigate();
  if (!me) return null;
  return (
    <div className="grid gap-2">
      <PageHeader
        eyebrow="Account"
        title="Settings"
        description="Your profile, appearance and security, and how this server is set up."
      />
      <Tabs
        value={tab}
        onValueChange={(value) =>
          void navigate({ search: value === "system" ? { tab: "system" } : {}, replace: true })
        }
      >
        <TabsList aria-label="Settings sections">
          <TabsTrigger value="account">Account</TabsTrigger>
          <TabsTrigger value="system">System</TabsTrigger>
        </TabsList>
        <TabsContent value="account" className="pt-0">
          {/* Keyed by user so the form resets if the account changes. */}
          <ProfileSection key={me.id} me={me} />
          <AppearanceSection />
          <PasswordSection />
          <DangerSection email={me.email} />
          {meta.data ? (
            <p className="pt-4 font-mono text-2xs text-fg-subtle">
              Akasha server {meta.data.version}
            </p>
          ) : null}
        </TabsContent>
        <TabsContent value="system" className="pt-0">
          <SystemPanel />
        </TabsContent>
      </Tabs>
    </div>
  );
}
