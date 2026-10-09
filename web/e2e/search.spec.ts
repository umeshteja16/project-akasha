import { expect, test } from "@playwright/test";
import { register, uniqueEmail, watchConsole } from "./support";

const HERON =
  "Field notes\n\nThe heron returned to the north pond at dawn, carrying reeds for its nest.\n" +
  "Later the kingfisher hunted along the reed bank while frost melted on the alders.\n";
const LEASE =
  "# Apartment lease\n\nThe lease renews automatically on March 1 unless either party gives " +
  "sixty days notice in writing. Rent is 1450 euros per month, due on the first.\n";

test("search finds uploaded passages, highlights them and opens the file there", async ({
  page,
}) => {
  const problems = watchConsole(page);
  await register(page, uniqueEmail("search"), "Search Tester");
  await page.getByTestId("upload-input").setInputFiles([
    { name: "heron-notes.txt", mimeType: "text/plain", buffer: Buffer.from(HERON) },
    { name: "lease.md", mimeType: "text/markdown", buffer: Buffer.from(LEASE) },
  ]);
  const main = page.locator("main");
  await expect(main.getByRole("link", { name: "lease.md" })).toBeVisible();
  await expect(main.getByText(/^(Queued|Reading)$/)).toHaveCount(0, { timeout: 20_000 });

  // The library's search box (focused with "/") opens the search screen.
  await page.keyboard.press("/");
  await page.keyboard.type("heron");
  await page.keyboard.press("Enter");
  await expect(page).toHaveURL(/\/search\?q=heron$/);
  // Results follow typing (debounced, kept in the URL).
  const box = page.getByRole("searchbox", { name: "Search your library" });
  await box.fill("heron dawn");
  await expect(page).toHaveURL(/\/search\?q=heron(\+|%20)dawn/);
  const results = page.getByRole("region", { name: "Results" });
  await expect(results.getByRole("link", { name: "heron-notes.txt" })).toBeVisible();
  await expect(results.locator("mark", { hasText: /^heron$/i }).first()).toBeVisible();
  await expect(results.getByRole("article").first()).toContainText("heron-notes.txt");

  // A filter narrows the search and is part of the URL; back undoes it.
  await page.getByRole("button", { name: "PDFs" }).click();
  await expect(page).toHaveURL(/type=pdf/);
  await expect(page.getByRole("heading", { name: /Nothing matched/ })).toBeVisible();
  await page.goBack();
  await expect(results.getByRole("link", { name: "heron-notes.txt" })).toBeVisible();

  // Opening a passage shows the file's text with the passage marked.
  await results
    .getByRole("list", { name: "Passages from heron-notes.txt" })
    .getByRole("link")
    .first()
    .click();
  await expect(page).toHaveURL(/\/files\/[0-9a-f-]+\?at=\d+-\d+/);
  await expect(page.getByRole("heading", { level: 1, name: "heron-notes.txt" })).toBeVisible();
  await expect(page.getByRole("tab", { name: "Text" })).toHaveAttribute("aria-selected", "true");
  await expect(page.locator("mark[data-passage]")).toContainText("heron returned");

  // The palette searches too.
  await page.keyboard.press("Control+k");
  await page.getByRole("combobox").fill("lease renews");
  const palette = page.getByRole("dialog");
  await expect(palette.getByRole("option", { name: /lease\.md/ })).toBeVisible();
  await palette.getByRole("option", { name: /lease\.md/ }).click();
  await expect(page.getByRole("heading", { level: 1, name: "lease.md" })).toBeVisible();

  expect(problems).toEqual([]);
});
