import { expect, test } from "@playwright/test";
import { register, uniqueEmail, watchConsole } from "./support";

const LEASE =
  "# Apartment lease\n\nThe lease renews automatically on March 1 unless either party gives " +
  "sixty days notice in writing. Rent is 1450 euros per month, due on the first.\n";
const HERON =
  "Field notes\n\nThe heron returned to the north pond at dawn, carrying reeds for its nest.\n";

test("chat: streamed answer with citations, refusal, rename and delete", async ({ page }) => {
  const problems = watchConsole(page);
  await register(page, uniqueEmail("chat"), "Chat Tester");
  await page.getByTestId("upload-input").setInputFiles([
    { name: "lease.md", mimeType: "text/markdown", buffer: Buffer.from(LEASE) },
    { name: "heron-notes.txt", mimeType: "text/plain", buffer: Buffer.from(HERON) },
  ]);
  const main = page.locator("main");
  await expect(main.getByText(/^(Queued|Reading)$/)).toHaveCount(0, { timeout: 20_000 });

  // Ask from a new chat: Enter sends; the answer streams in with citation chips.
  await page
    .getByRole("navigation", { name: "Main" })
    .first()
    .getByRole("link", { name: "Chat" })
    .click();
  await expect(page.getByRole("heading", { name: "What would you like to know?" })).toBeVisible();
  const box = page.getByRole("textbox", { name: "Your question" });
  await box.fill("When does the lease renew?");
  await box.press("Enter");
  await expect(page).toHaveURL(/\/chat\/[0-9a-f-]+$/);
  await expect(main.getByText("When does the lease renew?")).toBeVisible();
  const answer = main.getByRole("article", { name: "Answer" }).last();
  const chip = answer.getByRole("button", { name: /^Source 1: lease\.md/ });
  await expect(chip).toBeVisible({ timeout: 15_000 });
  await expect(answer.getByRole("button", { name: /sources? from/ })).toBeVisible();

  // The conversation is listed (titled after the question).
  const list = page.getByRole("navigation", { name: "Conversations" });
  await expect(list.getByRole("link", { name: /lease renew/i })).toBeVisible();

  // Hovering a citation shows the quote; clicking opens the file at the passage.
  await chip.hover();
  await expect(page.getByRole("dialog").getByText(/renews automatically/)).toBeVisible();
  await chip.click();
  await expect(page).toHaveURL(/\/files\/[0-9a-f-]+\?at=\d+-\d+/);
  await expect(page.getByRole("heading", { level: 1, name: "lease.md" })).toBeVisible();
  await expect(page.locator("mark[data-passage]")).toContainText("renews automatically");
  await page.goBack();
  await expect(page).toHaveURL(/\/chat\/[0-9a-f-]+$/);
  await expect(main.getByRole("button", { name: /^Source 1: lease\.md/ })).toBeVisible();

  // A question the files can't answer is refused, not guessed.
  await box.fill("What is the airspeed velocity of an unladen swallow?");
  await box.press("Enter");
  await expect(main.getByText("I couldn't find this in your files.")).toBeVisible({
    timeout: 15_000,
  });
  await expect(main.getByRole("link", { name: "search for related passages" })).toBeVisible();

  // Rename from the list, then delete.
  await list
    .getByRole("button", { name: /^Actions for / })
    .first()
    .click();
  await page.getByRole("menuitem", { name: "Rename" }).click();
  await page.getByRole("dialog").getByLabel("Title").fill("Lease questions");
  await page.getByRole("button", { name: "Save title" }).click();
  await expect(list.getByRole("link", { name: /Lease questions/ })).toBeVisible();
  await expect(page.getByRole("heading", { level: 1, name: "Lease questions" })).toBeVisible();

  await page.getByRole("button", { name: "Conversation actions" }).click();
  await page.getByRole("menuitem", { name: "Delete…" }).click();
  await page.getByRole("button", { name: "Delete forever" }).click();
  await expect(page).toHaveURL(/\/chat$/);
  await expect(list.getByRole("link", { name: /Lease questions/ })).toHaveCount(0);
  await expect(list.getByText(/Nothing asked yet/)).toBeVisible();

  // "Ask about these" from the library scopes a new chat to the selection.
  await page
    .getByRole("navigation", { name: "Main" })
    .first()
    .getByRole("link", { name: "Library" })
    .click();
  await page.getByRole("checkbox", { name: "Select heron-notes.txt" }).check();
  await page.getByRole("button", { name: /Ask about this/ }).click();
  await expect(page).toHaveURL(/\/chat\?files=/);
  await expect(main.getByText(/Answering only from/)).toContainText("heron-notes.txt");

  expect(problems).toEqual([]);
});
