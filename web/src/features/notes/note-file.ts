/** A note typed into the app, as a Markdown file ready for the uploader. */
export function noteFile(text: string, now: Date = new Date()): File {
  const body = text.trim();
  const stamp = now.toISOString().slice(0, 16).replace("T", " ").replace(":", "-");
  const title = body
    .split("\n", 1)[0]
    ?.replace(/^#+\s*/, "")
    .replace(/[^\p{L}\p{N} _-]+/gu, "")
    .trim()
    .slice(0, 40);
  const name = `${title || "Note"} ${stamp}.md`;
  return new File([`${body}\n`], name, { type: "text/markdown" });
}
