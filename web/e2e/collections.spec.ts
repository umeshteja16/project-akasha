// Collections, the activity timeline and Settings → Security, end to end.

import { expect, test } from "@playwright/test";
import { PASSWORD, register, uniqueEmail, watchConsole } from "./support";

const HERON =
  "Field notes\n\nThe heron returned to the north pond at dawn, carrying reeds for its nest.\n";
const LEASE =
  "# Apartment lease\n\nThe heron sculpture in the hall stays. The lease renews on March 1 " +
  "unless either party gives sixty days notice.\n";

// One account for everything: credential endpoints are rate-limited per IP.
test("collections, activity and sessions", async ({ page, browser }) => {
  test.setTimeout(90_000);
  const problems = watchConsole(page);
  const email = uniqueEmail("collections");
  await register(page, email, "Collector");
  await page.getByTestId("upload-input").setInputFiles([
    { name: "heron-notes.txt", mimeType: "text/plain", buffer: Buffer.from(HERON) },
    { name: "lease.md", mimeType: "text/markdown", buffer: Buffer.from(LEASE) },
  ]);
  const main = page.locator("main");
  await expect(main.getByRole("link", { name: "lease.md" })).toBeVisible();
  await expect(main.getByText(/^(Queued|Reading)$/)).toHaveCount(0, { timeout: 20_000 });

  // Library multi-select → Add to collection → New collection…
  await main.getByLabel("Select heron-notes.txt").check();
  await page.getByRole("button", { name: /Add to collection|Collect/ }).click();
  await page.getByRole("menuitem", { name: "New collection…" }).click();
  const dialog = page.getByRole("dialog", { name: "New collection" });
  await dialog.getByLabel("Name").fill("Birdwatching");
  // Click the swatch (the label): the radio itself is visually hidden, and CI's
  // browser refused to click it ("outside of the viewport") even with force.
  await dialog.locator("label", { has: page.getByRole("radio", { name: "Sky" }) }).click();
  await expect(dialog.getByRole("radio", { name: "Sky" })).toBeChecked();
  await dialog.getByRole("button", { name: "Create collection" }).click();
  await expect(page.getByText("Created “Birdwatching” with 1 file", { exact: true })).toBeVisible();

  // The sidebar lists it; its page shows the file.
  await page
    .getByRole("navigation", { name: "Main" })
    .getByRole("link", { name: /Birdwatching/ })
    .click();
  await expect(page.getByRole("heading", { level: 1, name: "Birdwatching" })).toBeVisible();
  await expect(main.getByRole("link", { name: "heron-notes.txt" })).toBeVisible();
  await expect(main.getByRole("link", { name: "lease.md" })).toHaveCount(0);

  // Add the other file with the dialog, then take it out again.
  await page.getByRole("button", { name: "Add files" }).first().click();
  const picker = page.getByRole("dialog", { name: /Add files to/ });
  await expect(picker.getByText("Already in this collection")).toBeVisible();
  await picker.getByRole("checkbox", { name: /lease\.md/ }).check();
  await picker.getByRole("button", { name: "Add", exact: true }).click();
  await expect(main.getByRole("link", { name: "lease.md" })).toBeVisible();
  await main.getByLabel("Select lease.md").check();
  await page.getByRole("button", { name: /Remove from collection|^Remove$/ }).click();
  await expect(main.getByRole("link", { name: "lease.md" })).toHaveCount(0);

  // Search within the collection: "heron" is in both files, only one is in it.
  await page.getByRole("link", { name: "Search in it" }).click();
  await expect(page).toHaveURL(/collection=/);
  await page.getByRole("searchbox", { name: "Search your library" }).fill("heron");
  const results = page.getByRole("region", { name: "Results" });
  await expect(results.getByRole("link", { name: "heron-notes.txt" })).toBeVisible();
  await expect(results.getByRole("link", { name: "lease.md" })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Birdwatching" })).toHaveAttribute(
    "aria-pressed",
    "true",
  );

  // Opening a file is tracked: "Recently opened" and the timeline.
  await results.getByRole("link", { name: "heron-notes.txt" }).first().click();
  await expect(page.getByRole("heading", { level: 1, name: "heron-notes.txt" })).toBeVisible();
  await expect(main.getByRole("list", { name: "In collections" })).toContainText("Birdwatching");

  await page.goto("/activity");
  await expect(page.getByRole("heading", { level: 1, name: "Activity" })).toBeVisible();
  const today = main.getByRole("region", { name: "Today" });
  for (const text of [
    "Opened heron-notes.txt",
    "Searched for “heron”",
    "Removed lease.md from Birdwatching",
    "Created the collection Birdwatching",
    "Added lease.md",
    "Created your account",
  ]) {
    await expect(today.locator("li", { hasText: text }).first()).toBeVisible();
  }
  await page.getByRole("button", { name: "Security" }).click();
  await expect(page).toHaveURL(/category=security/);
  await expect(main.locator("li", { hasText: "Opened heron-notes.txt" })).toHaveCount(0);
  await expect(main.locator("li", { hasText: "Created your account" })).toBeVisible();

  // Settings → Security: sign out another session.

  // A second device signs in.
  const other = await browser.newContext({
    userAgent: "Mozilla/5.0 (Android 15; Mobile) Firefox/140.0",
  });
  const login = await other.request.post("/api/v1/auth/login", {
    data: { email, password: PASSWORD },
  });
  expect(login.status()).toBe(200);
  expect((await other.request.get("/api/v1/me")).status()).toBe(200);

  await page.goto("/settings?tab=security");
  const sessions = page.getByRole("region", { name: "Where you're signed in" });
  await expect(sessions.getByText("This device")).toBeVisible();
  await expect(sessions.getByText("Firefox on Android")).toBeVisible();
  const signIns = page.getByRole("region", { name: "Recent sign-ins" });
  await expect(signIns.getByText(/Signed in/).first()).toBeVisible();

  await sessions.getByRole("button", { name: "Sign out Firefox on Android" }).click();
  await expect(page.getByText("Signed out that session", { exact: true })).toBeVisible();
  await expect(sessions.getByText("Firefox on Android")).toHaveCount(0);
  expect((await other.request.get("/api/v1/me")).status()).toBe(401);
  // This device is still signed in.
  expect((await page.request.get("/api/v1/me")).status()).toBe(200);

  // Search history can be switched off.
  const keep = page.getByRole("checkbox", { name: /Keep a history of my searches/ });
  await expect(keep).toBeChecked();
  await keep.uncheck();
  await expect(page.getByText("Searches won't be kept", { exact: true })).toBeVisible();

  await other.close();
  expect(problems).toEqual([]);
});
