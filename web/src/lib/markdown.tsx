// A small, safe Markdown renderer for previews. It builds React elements
// directly (never `innerHTML`), so raw HTML in a document shows as text. It
// covers what notes use: headings, paragraphs, lists, quotes, code, rules,
// emphasis, inline code and links (http, https and mailto only).

import { Fragment, type ReactNode } from "react";
import { cn } from "@/lib/utils";

export type Block =
  | { kind: "heading"; level: 1 | 2 | 3 | 4 | 5 | 6; text: string }
  | { kind: "paragraph"; text: string }
  | { kind: "code"; text: string; lang: string }
  | { kind: "quote"; text: string }
  | { kind: "list"; ordered: boolean; items: string[] }
  | { kind: "rule" };

const FENCE = /^(```|~~~)\s*([\w+-]*)\s*$/;
const HEADING = /^(#{1,6})\s+(.*?)\s*#*\s*$/;
const BULLET = /^\s*[-*+]\s+(.*)$/;
const ORDERED = /^\s*\d+[.)]\s+(.*)$/;
const RULE = /^\s*([-*_])(\s*\1){2,}\s*$/;
const QUOTE = /^\s*>\s?(.*)$/;

/** Split Markdown into blocks. */
export function parseBlocks(source: string): Block[] {
  const lines = source.replace(/\r\n?/g, "\n").split("\n");
  const blocks: Block[] = [];
  let i = 0;
  while (i < lines.length) {
    const line = lines[i] ?? "";
    if (!line.trim()) {
      i += 1;
      continue;
    }
    const fence = FENCE.exec(line);
    if (fence) {
      const body: string[] = [];
      i += 1;
      while (i < lines.length && !(lines[i] ?? "").startsWith(fence[1] ?? "```")) {
        body.push(lines[i] ?? "");
        i += 1;
      }
      i += 1;
      blocks.push({ kind: "code", text: body.join("\n"), lang: fence[2] ?? "" });
      continue;
    }
    const heading = HEADING.exec(line);
    if (heading) {
      const level = (heading[1]?.length ?? 1) as 1 | 2 | 3 | 4 | 5 | 6;
      blocks.push({ kind: "heading", level, text: heading[2] ?? "" });
      i += 1;
      continue;
    }
    if (RULE.test(line)) {
      blocks.push({ kind: "rule" });
      i += 1;
      continue;
    }
    if (QUOTE.test(line)) {
      const body: string[] = [];
      while (i < lines.length && QUOTE.test(lines[i] ?? "")) {
        body.push(QUOTE.exec(lines[i] ?? "")?.[1] ?? "");
        i += 1;
      }
      blocks.push({ kind: "quote", text: body.join(" ") });
      continue;
    }
    const ordered = ORDERED.test(line);
    if (ordered || BULLET.test(line)) {
      const pattern = ordered ? ORDERED : BULLET;
      const items: string[] = [];
      while (i < lines.length && pattern.test(lines[i] ?? "")) {
        items.push(pattern.exec(lines[i] ?? "")?.[1] ?? "");
        i += 1;
        // Continuation lines (indented, not a new item) join the item.
        while (
          i < lines.length &&
          /^\s{2,}\S/.test(lines[i] ?? "") &&
          !pattern.test(lines[i] ?? "")
        ) {
          items[items.length - 1] += ` ${(lines[i] ?? "").trim()}`;
          i += 1;
        }
      }
      blocks.push({ kind: "list", ordered, items });
      continue;
    }
    const body: string[] = [];
    while (i < lines.length) {
      const next = lines[i] ?? "";
      if (!next.trim() || FENCE.test(next) || HEADING.test(next) || QUOTE.test(next)) break;
      if (BULLET.test(next) || ORDERED.test(next) || RULE.test(next)) break;
      body.push(next.trim());
      i += 1;
    }
    blocks.push({ kind: "paragraph", text: body.join(" ") });
  }
  return blocks;
}

const SAFE_URL = /^(https?:|mailto:)/i;
// `code`, **strong**, __strong__, *em*, _em_, [text](url)
const INLINE =
  /(`[^`]+`)|(\*\*[^*]+\*\*|__[^_]+__)|(\*[^*\s][^*]*\*|_[^_\s][^_]*_)|(\[[^\]]+\]\([^)\s]+\))/;

/** Renders a run of plain text (e.g. to turn `[n]` into citation chips). */
export type TextRenderer = (text: string, key: string) => ReactNode;

/** Render inline Markdown as React nodes. */
export function renderInline(
  text: string,
  keyPrefix = "i",
  renderText?: TextRenderer,
): ReactNode[] {
  const out: ReactNode[] = [];
  let rest = text;
  let n = 0;
  const plain = (t: string) => (renderText ? renderText(t, `${keyPrefix}-t${n++}`) : t);
  while (rest) {
    const match = INLINE.exec(rest);
    if (!match) {
      out.push(plain(rest));
      break;
    }
    if (match.index > 0) out.push(plain(rest.slice(0, match.index)));
    const token = match[0];
    const key = `${keyPrefix}-${n++}`;
    if (match[1]) {
      out.push(
        <code key={key} className="rounded-xs bg-surface-2 px-1 py-0.5 font-mono text-[0.9em]">
          {token.slice(1, -1)}
        </code>,
      );
    } else if (match[2]) {
      out.push(<strong key={key}>{renderInline(token.slice(2, -2), key, renderText)}</strong>);
    } else if (match[3]) {
      out.push(<em key={key}>{renderInline(token.slice(1, -1), key, renderText)}</em>);
    } else {
      const link = /^\[([^\]]+)\]\(([^)\s]+)\)$/.exec(token);
      const label = link?.[1] ?? token;
      const href = link?.[2] ?? "";
      out.push(
        SAFE_URL.test(href) ? (
          <a
            key={key}
            href={href}
            target="_blank"
            rel="noopener noreferrer nofollow"
            className="text-accent-text underline underline-offset-2"
          >
            {renderInline(label, key)}
          </a>
        ) : (
          <Fragment key={key}>{renderInline(label, key)}</Fragment>
        ),
      );
    }
    rest = rest.slice(match.index + token.length);
  }
  return out;
}

const HEADING_CLASS: Record<number, string> = {
  1: "display text-2xl",
  2: "display text-xl",
  3: "text-lg font-semibold",
  4: "text-base font-semibold",
  5: "text-sm font-semibold",
  6: "text-sm font-semibold text-fg-muted",
};

/** Markdown as a styled, safe React tree. */
export function Markdown({
  source,
  renderText,
  className,
}: {
  source: string;
  renderText?: TextRenderer;
  className?: string;
}) {
  const inline = (text: string, key: string) => renderInline(text, key, renderText);
  return (
    <div className={cn("grid gap-4 text-base text-fg [overflow-wrap:anywhere]", className)}>
      {parseBlocks(source).map((block, index) => {
        const key = `b${index}`;
        switch (block.kind) {
          case "heading": {
            const Tag = `h${Math.min(block.level + 1, 6)}` as "h2";
            return (
              <Tag key={key} className={`${HEADING_CLASS[block.level]} mt-2 text-fg`}>
                {inline(block.text, key)}
              </Tag>
            );
          }
          case "paragraph":
            return <p key={key}>{inline(block.text, key)}</p>;
          case "code":
            return (
              <pre
                key={key}
                className="overflow-x-auto rounded-md bg-surface-2 p-3 font-mono text-xs leading-relaxed"
              >
                <code>{block.text}</code>
              </pre>
            );
          case "quote":
            return (
              <blockquote
                key={key}
                className="border-l-2 border-border-strong/50 pl-4 text-fg-muted italic"
              >
                {inline(block.text, key)}
              </blockquote>
            );
          case "list": {
            const List = block.ordered ? "ol" : "ul";
            return (
              <List
                key={key}
                className={`grid gap-1 pl-6 ${block.ordered ? "list-decimal" : "list-disc"}`}
              >
                {block.items.map((item, i) => (
                  // biome-ignore lint/suspicious/noArrayIndexKey: list items have no identity
                  <li key={i}>{inline(item, `${key}-${i}`)}</li>
                ))}
              </List>
            );
          }
          case "rule":
            return <hr key={key} className="border-border" />;
        }
        return null;
      })}
    </div>
  );
}
