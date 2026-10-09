import { expect, test } from "@playwright/test";
import { register, uniqueEmail, watchConsole } from "./support";

// A valid 1×1 PNG.
const PNG = Buffer.from(
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==",
  "base64",
);
const NOTE = "Field notes\n\nThe heron returned to the north pond at dawn, carrying reeds.\n";

test("upload → ready → detail → rename, tag, pin → delete", async ({ page }) => {
  const problems = watchConsole(page);
  await register(page, uniqueEmail("library"), "Grace Hopper");

  // The empty library teaches what to do.
  await expect(page.getByRole("heading", { name: "Drop your first files here" })).toBeVisible();

  // Upload two files through the Upload button's file input.
  await page.getByTestId("upload-input").setInputFiles([
    { name: "heron-notes.txt", mimeType: "text/plain", buffer: Buffer.from(NOTE) },
    { name: "pixel.png", mimeType: "image/png", buffer: PNG },
  ]);
  const uploads = page.getByRole("region", { name: "Uploads" });
  await expect(uploads.getByText("Added, now reading it.")).toHaveCount(2);

  // Both show up in the library at once and finish processing (polling).
  const main = page.locator("main");
  await expect(main.getByRole("link", { name: "heron-notes.txt" })).toBeVisible();
  await expect(main.getByRole("link", { name: "pixel.png" })).toBeVisible();
  await expect(main.getByText(/^(Queued|Reading)$/)).toHaveCount(0, { timeout: 20_000 });

  // Uploading the same bytes again is recognised as a duplicate.
  await page.getByTestId("upload-input").setInputFiles({
    name: "copy.png",
    mimeType: "image/png",
    buffer: PNG,
  });
  await expect(uploads.getByText("Already in your library.")).toBeVisible();

  // The image's page previews it inline.
  await main.getByRole("link", { name: "pixel.png" }).click();
  await expect(page).toHaveURL(/\/files\/[0-9a-f-]+$/);
  await expect(page.getByRole("heading", { level: 1, name: "pixel.png" })).toBeVisible();
  await expect(page.getByRole("tabpanel").getByRole("img")).toBeVisible();
  await page.getByRole("link", { name: "Library" }).first().click();

  // The text file: ready, with its extracted text.
  await main.getByRole("link", { name: "heron-notes.txt" }).click();
  await expect(page.getByRole("heading", { level: 1, name: "heron-notes.txt" })).toBeVisible();
  await expect(page.locator("header").getByText("Ready", { exact: true })).toBeVisible();
  await page.getByRole("tab", { name: "Text" }).click();
  await expect(
    page.getByRole("tabpanel").getByText(/heron returned to the north pond/),
  ).toBeVisible();

  // Rename.
  await page.getByRole("button", { name: "More actions" }).click();
  await page.getByRole("menuitem", { name: "Rename" }).click();
  await page.getByRole("dialog").getByLabel("Name", { exact: true }).fill("heron-field-notes.txt");
  await page.getByRole("button", { name: "Save name" }).click();
  await expect(
    page.getByRole("heading", { level: 1, name: "heron-field-notes.txt" }),
  ).toBeVisible();

  // Tag.
  await page.getByLabel("Add a tag").fill("birds");
  await page.getByLabel("Add a tag").press("Enter");
  await expect(page.getByRole("list", { name: "Your tags" })).toContainText("birds");

  // Pin.
  await page.getByRole("button", { name: "Pin", exact: true }).click();
  await expect(page.getByRole("button", { name: "Pinned" })).toHaveAttribute(
    "aria-pressed",
    "true",
  );

  // The library shows it under the Pinned filter, with its new name and tag.
  await page.getByRole("link", { name: "Library" }).first().click();
  await page.getByRole("button", { name: "Pinned" }).click();
  await expect(page).toHaveURL(/pinned=true/);
  await expect(main.getByRole("link", { name: "heron-field-notes.txt" })).toBeVisible();
  await expect(main.getByRole("link", { name: "pixel.png" })).toHaveCount(0);
  await expect(main.getByText("birds").first()).toBeVisible();

  // Delete from the file page, with confirmation.
  await main.getByRole("link", { name: "heron-field-notes.txt" }).click();
  await page.keyboard.press("Delete");
  await expect(page.getByRole("dialog")).toContainText("This can't be undone");
  await page.getByRole("button", { name: "Delete forever" }).click();
  await expect(page).toHaveURL(/\/library/);
  await expect(page.getByText("File deleted", { exact: true })).toBeVisible();
  await expect(main.getByRole("link", { name: "heron-field-notes.txt" })).toHaveCount(0);

  expect(problems).toEqual([]);
});

test("inline downloads are limited to viewable types", async ({ page }) => {
  await register(page, uniqueEmail("inline"), "Inline Tester");
  const upload = await page.request.post("/api/v1/files", {
    multipart: { file: { name: "pixel.png", mimeType: "image/png", buffer: PNG } },
  });
  expect(upload.status()).toBe(201);
  const { id } = (await upload.json()) as { id: string };

  const inline = await page.request.get(`/api/v1/files/${id}/download?inline=true`);
  expect(inline.headers()["content-disposition"]).toMatch(/^inline/);
  expect(inline.headers()["content-type"]).toBe("image/png");
  expect(inline.headers()["x-content-type-options"]).toBe("nosniff");

  const attachment = await page.request.get(`/api/v1/files/${id}/download`);
  expect(attachment.headers()["content-disposition"]).toMatch(/^attachment/);
});
