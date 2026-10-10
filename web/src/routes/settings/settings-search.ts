export type SettingsTab = "system" | "tokens" | "security" | "sources";

export interface SettingsSearch {
  /** `security`, `sources`, `tokens` or `system` opens that tab; the account tab is the default. */
  tab?: SettingsTab;
}

const TABS: readonly SettingsTab[] = ["system", "tokens", "security", "sources"];

export function validateSettingsSearch(search: Record<string, unknown>): SettingsSearch {
  const tab = TABS.find((t) => t === search.tab);
  return tab ? { tab } : {};
}
