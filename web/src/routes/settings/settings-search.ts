export interface SettingsSearch {
  /** `system` opens the System tab; the account tab is the default. */
  tab?: "system";
}

export function validateSettingsSearch(search: Record<string, unknown>): SettingsSearch {
  return search.tab === "system" ? { tab: "system" } : {};
}
