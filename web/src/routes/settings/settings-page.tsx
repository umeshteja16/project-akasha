import { useQuery } from "@tanstack/react-query";
import { useApi } from "@/api/context";
import { meQuery, metaQuery } from "@/api/queries";
import { PageHeader } from "@/components/common/page-header";
import { AppearanceSection } from "./appearance-section";
import { DangerSection } from "./danger-section";
import { PasswordSection } from "./password-section";
import { ProfileSection } from "./profile-section";

export function SettingsPage() {
  const api = useApi();
  const { data: me } = useQuery(meQuery(api));
  const meta = useQuery(metaQuery(api));
  if (!me) return null;
  return (
    <div className="grid gap-2">
      <PageHeader
        eyebrow="Account"
        title="Settings"
        description="Your profile, appearance and security."
      />
      {/* Keyed by user so the form resets if the account changes. */}
      <ProfileSection key={me.id} me={me} />
      <AppearanceSection />
      <PasswordSection />
      <DangerSection email={me.email} />
      {meta.data ? (
        <p className="pt-4 font-mono text-2xs text-fg-subtle">Akasha server {meta.data.version}</p>
      ) : null}
    </div>
  );
}
