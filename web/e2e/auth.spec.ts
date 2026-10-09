import { expect, type Page, test } from "@playwright/test";
import { PASSWORD, register, uniqueEmail, watchConsole } from "./support";

const NEW_PASSWORD = "staple battery horse correct";

async function signOut(page: Page) {
  await page.getByRole("button", { name: "Account menu" }).click();
  await page.getByRole("menuitem", { name: "Sign out" }).click();
  await expect(page).toHaveURL(/\/sign-in$/);
}

async function signIn(page: Page, email: string, password: string) {
  await page.getByLabel("Email").fill(email);
  await page.getByLabel("Password").fill(password);
  await page.getByRole("button", { name: "Sign in" }).click();
}

test("serves the SPA with security headers", async ({ request }) => {
  const response = await request.get("/settings");
  expect(response.status()).toBe(200);
  expect(response.headers()["content-type"]).toContain("text/html");
  expect(response.headers()["cache-control"]).toBe("no-cache");
  expect(response.headers()["content-security-policy"]).toContain("frame-ancestors 'none'");
  expect(response.headers()["x-frame-options"]).toBe("DENY");

  const api = await request.get("/api/v1/nope");
  expect(api.status()).toBe(404);
  expect((await api.json()).error.code).toBe("not_found");
});

test("register → settings → sign out → sign in", async ({ page }) => {
  const problems = watchConsole(page);
  const email = uniqueEmail("flow");
  await register(page, email, "Ada Lovelace");

  // The shell knows who we are.
  await expect(page.getByRole("button", { name: "Account menu" })).toContainText("Ada Lovelace");

  // Settings: rename.
  await page
    .getByRole("navigation", { name: "Main" })
    .getByRole("link", { name: "Settings" })
    .click();
  await expect(page).toHaveURL(/\/settings$/);
  await page.getByLabel("Display name").fill("Ada King");
  await page.getByRole("button", { name: "Save profile" }).click();
  await expect(page.getByText("Profile saved", { exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "Account menu" })).toContainText("Ada King");

  // Settings: theme is applied and survives a reload (also proves the SPA fallback
  // and the session cookie).
  await page.getByRole("radio", { name: "Dark" }).click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  await page.reload();
  await expect(page.getByRole("heading", { level: 1, name: "Settings" })).toBeVisible();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");

  // Settings: wrong current password, then a real change.
  await page.getByLabel("Current password").fill("not my password");
  await page.getByLabel("New password", { exact: true }).fill(NEW_PASSWORD);
  await page.getByLabel("Repeat new password").fill(NEW_PASSWORD);
  await page.getByRole("button", { name: "Change password" }).click();
  await expect(page.getByRole("alert")).toContainText("current password is incorrect");
  await expect(page).toHaveURL(/\/settings$/); // a wrong password is not a sign-out
  await page.getByLabel("Current password").fill(PASSWORD);
  await page.getByRole("button", { name: "Change password" }).click();
  await expect(page.getByText("Password changed", { exact: true })).toBeVisible();

  await signOut(page);

  // Signed out: app routes bounce to sign-in and come back after signing in.
  await page.goto("/chat");
  await expect(page).toHaveURL(/\/sign-in\?redirect=%2Fchat/);
  await signIn(page, email, PASSWORD);
  await expect(page.getByRole("alert")).toContainText("don't match");
  await signIn(page, email, NEW_PASSWORD);
  await expect(page).toHaveURL(/\/chat$/);
  await expect(page.getByRole("heading", { level: 1, name: "New conversation" })).toBeVisible();

  expect(problems).toEqual([]);
});

test("delete account", async ({ page }) => {
  const email = uniqueEmail("delete");
  await register(page, email, "Temporary");
  await page.goto("/settings");
  await page.getByRole("button", { name: "Delete account…" }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog.getByRole("heading", { name: "Delete your account?" })).toBeVisible();
  await dialog.getByLabel("Password").fill(PASSWORD);
  await dialog.getByLabel('Type "delete" to confirm').fill("delete");
  await dialog.getByRole("button", { name: "Delete forever" }).click();
  await expect(page).toHaveURL(/\/sign-in$/);
  await expect(page.getByText("Your account has been deleted", { exact: true })).toBeVisible();

  await signIn(page, email, PASSWORD);
  await expect(page.getByRole("alert")).toContainText("don't match");
});

test("unknown pages show the 404 screen", async ({ page }) => {
  await page.goto("/definitely/not/here");
  await expect(
    page.getByRole("heading", { name: "This page was never written down" }),
  ).toBeVisible();
});
