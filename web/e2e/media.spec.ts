// Recordings: an uploaded WAV is transcribed (by the server's deterministic
// "tone" model), found by search with its time, and opened at that time with
// the transcript next to the player; timestamps seek the player.

import AxeBuilder from "@axe-core/playwright";
import { expect, test } from "@playwright/test";
import { register, uniqueEmail, watchConsole } from "./support";

/** Mono 16-bit WAV: 3 s at 440 Hz, 1 s silence, 3 s at 880 Hz (16 kHz). */
function tonesWav(): Buffer {
  const rate = 16_000;
  const samples: number[] = [];
  const tone = (freq: number, seconds: number) => {
    for (let i = 0; i < rate * seconds; i++) {
      samples.push(freq ? Math.round(Math.sin((2 * Math.PI * freq * i) / rate) * 12_000) : 0);
    }
  };
  tone(440, 3);
  tone(0, 1);
  tone(880, 3);
  const data = samples.length * 2;
  const out = Buffer.alloc(44 + data);
  out.write("RIFF", 0);
  out.writeUInt32LE(36 + data, 4);
  out.write("WAVEfmt ", 8);
  out.writeUInt32LE(16, 16);
  out.writeUInt16LE(1, 20);
  out.writeUInt16LE(1, 22);
  out.writeUInt32LE(rate, 24);
  out.writeUInt32LE(rate * 2, 28);
  out.writeUInt16LE(2, 32);
  out.writeUInt16LE(16, 34);
  out.write("data", 36);
  out.writeUInt32LE(data, 40);
  samples.forEach((s, i) => {
    out.writeInt16LE(s, 44 + i * 2);
  });
  return out;
}

test("recordings are transcribed, found with their time and played from there", async ({
  page,
}) => {
  const problems = watchConsole(page);
  await register(page, uniqueEmail("media"), "Media Tester");
  await page
    .getByTestId("upload-input")
    .setInputFiles([{ name: "tones.wav", mimeType: "audio/wav", buffer: tonesWav() }]);
  const main = page.locator("main");
  await expect(main.getByRole("link", { name: "tones.wav" })).toBeVisible();
  await expect(main.getByText(/^(Queued|Reading)$/)).toHaveCount(0, { timeout: 20_000 });

  await page.goto("/search?q=hertz");
  const results = page.getByRole("region", { name: "Results" });
  await expect(results.getByRole("link", { name: "tones.wav" })).toBeVisible();
  const passages = results.getByRole("list", { name: "Passages from tones.wav" });
  await expect(passages).toContainText("0:00");
  await passages.getByRole("link").first().click();
  await expect(page).toHaveURL(/\/files\/[0-9a-f-]+\?at=\d+-\d+&t=0/);

  const transcript = page.getByRole("region", { name: "Transcript" });
  await expect(transcript.getByText("tone 440 hertz")).toBeVisible();
  await expect(transcript.getByText("tone 880 hertz")).toBeVisible();
  await expect(transcript.locator("[data-passage]").first()).toBeVisible();
  await transcript.getByRole("button", { name: "Play from 0:04" }).click();
  await expect
    .poll(() => page.locator("audio").evaluate((el: HTMLAudioElement) => el.currentTime))
    .toBeGreaterThanOrEqual(3.5); // players seek to the nearest frame
  await expect(transcript.locator('li[aria-current="true"]')).toContainText("tone 880 hertz");

  for (const scheme of ["light", "dark"] as const) {
    await page.emulateMedia({ colorScheme: scheme });
    await page.waitForTimeout(350);
    const audit = await new AxeBuilder({ page })
      .withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa", "best-practice"])
      .analyze();
    expect(
      audit.violations.map((v) => `${v.id}: ${v.help}`),
      `axe on the recording page (${scheme})`,
    ).toEqual([]);
  }
  expect(problems).toEqual([]);
});
