// Accessibility: axe-core over every main screen in light and dark, plus the
// keyboard basics (skip link, focus, shortcuts, titles) and reduced motion. One
// account for all of it: credential endpoints are rate-limited per IP.

import AxeBuilder from "@axe-core/playwright";
import { expect, type Page, test } from "@playwright/test";
import { register, uniqueEmail, watchConsole } from "./support";

const MOON =
  "# Apollo 11\n\nThe lunar module Eagle landed in the Sea of Tranquility on 20 July 1969. " +
  "Armstrong and Aldrin walked on the Moon while Collins orbited.\n";
const BUDGET = "month,groceries,rent\nJanuary,310,1450\nFebruary,295,1450\n";

const SCHEMES = ["light", "dark"] as const;

/** WCAG 2.2 A/AA plus axe best practices (landmarks, headings). */
async function audit(page: Page, screen: string) {
  // Let entrance animations settle so colours are measured at rest.
  await page.waitForTimeout(350);
  const results = await new AxeBuilder({ page })
    .withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa", "best-practice"])
    .analyze();
  const summary = results.violations.map(
    (v) =>
      `${v.id} (${v.impact}): ${v.help}\n${v.nodes
        .slice(0, 5)
        .map((n) => `    ${n.target.join(" ")}: ${n.failureSummary?.split("\n")[1] ?? ""}`)
        .join("\n")}`,
  );
  expect(summary, `axe violations on ${screen}`).toEqual([]);
}

async function inBothSchemes(page: Page, screen: string, check?: () => Promise<void>) {
  for (const scheme of SCHEMES) {
    await page.emulateMedia({ colorScheme: scheme });
    await expect(page.locator("html")).toHaveAttribute("data-theme", scheme);
    await check?.();
    await audit(page, `${screen} (${scheme})`);
  }
}

test("every main screen passes axe in light and dark", async ({ page }) => {
  test.setTimeout(120_000);
  const problems = watchConsole(page);

  await page.goto("/sign-in");
  await inBothSchemes(page, "sign in");
  await page.goto("/register");
  await inBothSchemes(page, "register");

  await register(page, uniqueEmail("a11y"), "Ada Access");
  await keyboardBasics(page);
  await page.emulateMedia({ reducedMotion: "no-preference" });
  await page.goto("/library");
  const main = page.locator("main");
  await inBothSchemes(page, "first-run library", () =>
    expect(page.getByRole("heading", { name: "Drop your first files here" })).toBeVisible(),
  );

  await page.getByTestId("upload-input").setInputFiles([
    { name: "moon-landing.md", mimeType: "text/markdown", buffer: Buffer.from(MOON) },
    { name: "budget.csv", mimeType: "text/csv", buffer: Buffer.from(BUDGET) },
  ]);
  await expect(main.getByText(/^(Queued|Reading)$/)).toHaveCount(0, { timeout: 20_000 });
  await inBothSchemes(page, "library");

  await page.goto("/search?q=lunar+module+eagle");
  await expect(main.getByRole("link", { name: "moon-landing.md" })).toBeVisible();
  await inBothSchemes(page, "search");
  const weak = main.getByRole("button", { name: /loosely related/ });
  if (await weak.isVisible()) {
    await weak.click();
    await inBothSchemes(page, "search with loosely related");
  }

  await main.getByRole("link", { name: "moon-landing.md" }).first().click();
  await expect(page.getByRole("heading", { level: 1, name: "moon-landing.md" })).toBeVisible();
  await inBothSchemes(page, "file");

  await page.goto("/chat");
  await expect(page.getByRole("heading", { name: "What would you like to know?" })).toBeVisible();
  await inBothSchemes(page, "new chat");
  const box = page.getByRole("textbox", { name: "Your question" });
  await box.fill("Where did the lunar module land?");
  await box.press("Enter");
  const answer = main.getByRole("article", { name: "Answer" }).last();
  await expect(answer.getByRole("button", { name: /^Source 1/ })).toBeVisible({ timeout: 15_000 });
  await inBothSchemes(page, "conversation");

  await page.goto("/settings");
  await inBothSchemes(page, "settings");
  await page.getByRole("tab", { name: "System" }).click();
  await expect(page.getByText("hash-384")).toBeVisible();
  await inBothSchemes(page, "settings system");

  await page.goto("/library");
  await expect(page.getByRole("heading", { level: 1, name: "Library" })).toBeVisible();
  await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
  await page.keyboard.press("?");
  await expect(page.getByRole("dialog", { name: "Keyboard shortcuts" })).toBeVisible();
  await inBothSchemes(page, "shortcuts");
  await page.keyboard.press("Escape");

  await page.keyboard.press("Control+k");
  await expect(page.getByRole("dialog", { name: "Command palette" })).toBeVisible();
  await inBothSchemes(page, "command palette");
  await page.keyboard.press("Escape");

  await page.goto("/no/such/page");
  await inBothSchemes(page, "404");

  expect(problems).toEqual([]);
});

/** Skip link, visible focus, shortcuts, titles, in-app 404, reduced motion, manifest. */
async function keyboardBasics(page: Page) {
  await expect(page).toHaveTitle("Library · Akasha");

  // The skip link is the first tab stop and moves focus to the content.
  await page.keyboard.press("Tab");
  const skip = page.getByRole("link", { name: "Skip to content" });
  await expect(skip).toBeFocused();
  await expect(skip).toBeVisible();
  await page.keyboard.press("Enter");
  await expect(page.locator("main")).toBeFocused();

  // Focus is visible on controls reached by keyboard (an outline, or a ring on inputs).
  await page.keyboard.press("Tab");
  const focus = await page.evaluate(() => {
    const el = document.activeElement;
    if (!el || el === document.body) return "nothing focused";
    const style = getComputedStyle(el);
    return style.outlineStyle !== "none" || style.boxShadow !== "none" ? "visible" : el.outerHTML;
  });
  expect(focus).toBe("visible");

  // `g s` goes to search; the title follows the screen.
  await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
  await page.keyboard.press("g");
  await page.keyboard.press("s");
  await expect(page).toHaveURL(/\/search/);
  await expect(page).toHaveTitle("Search · Akasha");

  // Unknown files are a friendly 404 inside the app.
  await page.goto("/files/00000000-0000-4000-8000-000000000000");
  await expect(
    page.getByRole("heading", { name: "This file isn't in your library" }),
  ).toBeVisible();
  await expect(page).toHaveTitle("File not found · Akasha");

  // Reduced motion: the motion tokens collapse to zero.
  await page.emulateMedia({ reducedMotion: "reduce" });
  const duration = await page.evaluate(() =>
    getComputedStyle(document.documentElement).getPropertyValue("--motion-base").trim(),
  );
  expect(["0ms", "0s"]).toContain(duration);

  // The app is installable: the manifest and its icons are served.
  const manifest = await page.request.get("/manifest.webmanifest");
  expect(manifest.ok()).toBe(true);
  const body = (await manifest.json()) as { icons: { src: string }[] };
  for (const icon of body.icons) {
    expect((await page.request.get(icon.src)).ok(), icon.src).toBe(true);
  }
}
