// Shared helpers for the end-to-end tests.

import { expect, type Page } from "@playwright/test";

export const PASSWORD = "correct horse battery";

export function uniqueEmail(tag: string): string {
  return `e2e-${tag}-${Date.now()}-${Math.floor(Math.random() * 1e6)}@example.test`;
}

/** Fail the test on any CSP violation or uncaught page error. */
export function watchConsole(page: Page): string[] {
  const problems: string[] = [];
  page.on("console", (msg) => {
    if (msg.type() === "error" && /Content Security Policy|Refused to/i.test(msg.text())) {
      problems.push(msg.text());
    }
  });
  page.on("pageerror", (error) => problems.push(error.message));
  return problems;
}

export async function register(page: Page, email: string, name: string) {
  await page.goto("/");
  await expect(page).toHaveURL(/\/sign-in/);
  await page.getByRole("link", { name: "Create an account" }).click();
  await expect(page.getByRole("heading", { name: "Start your library" })).toBeVisible();
  await page.getByLabel("Name").fill(name);
  await page.getByLabel("Email").fill(email);
  await page.getByLabel("Password").fill(PASSWORD);
  await page.getByRole("button", { name: "Create account" }).click();
  await expect(page).toHaveURL(/\/library$/);
  await expect(page.getByRole("heading", { level: 1, name: "Library" })).toBeVisible();
}
