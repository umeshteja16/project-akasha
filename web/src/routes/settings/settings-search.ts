export type SettingsTab = "system" | "tokens";

export interface SettingsSearch {
  /** `system` or `tokens` opens that tab; the account tab is the default. */
  tab?: SettingsTab;
}

export function validateSettingsSearch(search: Record<string, unknown>): SettingsSearch {
  return search.tab === "system" || search.tab === "tokens" ? { tab: search.tab } : {};
}
