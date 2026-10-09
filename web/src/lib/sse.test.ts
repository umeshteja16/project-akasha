import { describe, expect, it } from "vitest";
import { readSse, type SseMessage, SseParser } from "./sse";

function parseAll(chunks: string[]): SseMessage[] {
  const out: SseMessage[] = [];
  const parser = new SseParser((m) => out.push(m));
  for (const chunk of chunks) parser.push(chunk);
  parser.end();
  return out;
}

const STREAM =
  'event: sources\ndata: {"sources":[]}\n\n' +
  ": keep-alive\n\n" +
  'event: delta\ndata: {"text":"Hel"}\n\n' +
  'event: delta\ndata: {"text":"lo"}\n\n' +
  'event: done\ndata: {"status":"answered"}\n\n';

const EXPECTED: SseMessage[] = [
  { event: "sources", data: '{"sources":[]}' },
  { event: "delta", data: '{"text":"Hel"}' },
  { event: "delta", data: '{"text":"lo"}' },
  { event: "done", data: '{"status":"answered"}' },
];

describe("SseParser", () => {
  it("parses several events in one chunk and skips comments", () => {
    expect(parseAll([STREAM])).toEqual(EXPECTED);
  });

  it("gives the same result however the stream is split", () => {
    for (let size = 1; size <= 9; size++) {
      const chunks: string[] = [];
      for (let i = 0; i < STREAM.length; i += size) chunks.push(STREAM.slice(i, i + size));
      expect(parseAll(chunks)).toEqual(EXPECTED);
    }
  });

  it("accepts CRLF and CR line endings, also split between chunks", () => {
    const crlf = STREAM.replaceAll("\n", "\r\n");
    expect(parseAll([crlf])).toEqual(EXPECTED);
    const parts: string[] = [];
    for (let i = 0; i < crlf.length; i += 1) parts.push(crlf.charAt(i));
    expect(parseAll(parts)).toEqual(EXPECTED);
    expect(parseAll([STREAM.replaceAll("\n", "\r")])).toEqual(EXPECTED);
  });

  it("joins multi-line data, defaults the event name and keeps ids", () => {
    expect(parseAll(["id: 7\ndata: one\ndata:two\ndata\n\n"])).toEqual([
      { event: "message", data: "one\ntwo\n", id: "7" },
    ]);
  });

  it("drops an unterminated event at the end and events without data", () => {
    expect(parseAll(["event: ping\n\n", 'event: delta\ndata: {"text":"cut'])).toEqual([]);
  });
});

describe("readSse", () => {
  function streamOf(parts: Uint8Array[]): ReadableStream<Uint8Array> {
    return new ReadableStream({
      start(controller) {
        for (const part of parts) controller.enqueue(part);
        controller.close();
      },
    });
  }

  it("decodes multi-byte characters split across chunks", async () => {
    const bytes = new TextEncoder().encode('event: delta\ndata: {"text":"héron 🦩"}\n\n');
    const cut = bytes.indexOf(0xc3) + 1; // inside "é"
    const emoji = bytes.indexOf(0xf0) + 2; // inside the emoji
    const out: SseMessage[] = [];
    await readSse(
      streamOf([bytes.slice(0, cut), bytes.slice(cut, emoji), bytes.slice(emoji)]),
      (m) => out.push(m),
    );
    expect(out).toEqual([{ event: "delta", data: '{"text":"héron 🦩"}' }]);
  });

  it("rejects with the stream's error", async () => {
    let pulls = 0;
    const failing = new ReadableStream<Uint8Array>({
      pull(controller) {
        pulls += 1;
        if (pulls === 1) controller.enqueue(new TextEncoder().encode("event: delta\ndata: {}\n\n"));
        else controller.error(new Error("connection reset"));
      },
    });
    const out: SseMessage[] = [];
    await expect(readSse(failing, (m) => out.push(m))).rejects.toThrow("connection reset");
    expect(out).toHaveLength(1);
  });
});
