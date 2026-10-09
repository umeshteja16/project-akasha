import { describe, expect, it } from "vitest";
import { ApiError, createApi } from "@/api/client";
import { apiError, fakeFetch } from "@/test/fetch";
import { type ChatEvent, decodeChatEvent, streamAnswer } from "./stream";

const BASE = "http://akasha.test";
const ID = "0b9c4f8e-0000-4000-8000-000000000001";
const PATH = `POST /api/v1/conversations/${ID}/messages`;

function sse(chunks: string[], opts: { error?: boolean } = {}): Response {
  const encoder = new TextEncoder();
  let i = 0;
  const body = new ReadableStream<Uint8Array>({
    pull(controller) {
      const chunk = chunks[i++];
      if (chunk !== undefined) controller.enqueue(encoder.encode(chunk));
      else if (opts.error) controller.error(new TypeError("network error"));
      else controller.close();
    },
  });
  return new Response(body, { headers: { "content-type": "text/event-stream" } });
}

const SOURCES = {
  conversation_id: ID,
  user_message_id: "u1",
  search_query: "heron",
  sources: [],
};
const DONE = {
  message_id: "m1",
  status: "answered",
  content: "Fact [1].",
  citations: [],
  usage: {},
  latency_ms: 12,
};

async function collect(response: () => Response): Promise<{ events: ChatEvent[]; body: string }> {
  let body = "";
  const { fetch } = fakeFetch({
    [PATH]: async (req) => {
      body = await req.text();
      return response();
    },
  });
  const api = createApi({ baseUrl: BASE, fetch });
  const events: ChatEvent[] = [];
  await streamAnswer(api, ID, { content: "Where?", file_ids: ["f1"] }, (e) => events.push(e));
  return { events, body };
}

describe("streamAnswer", () => {
  it("posts the question and delivers events split across chunks", async () => {
    const stream = `event: sources\ndata: ${JSON.stringify(SOURCES)}\n\nevent: delta\ndata: {"text":"Fact "}\n\n: ping\n\nevent: delta\ndata: {"text":"[1]."}\n\nevent: done\ndata: ${JSON.stringify(DONE)}\n\n`;
    const chunks = stream.match(/[\s\S]{1,7}/g) ?? [];
    const { events, body } = await collect(() => sse(chunks));
    expect(JSON.parse(body)).toEqual({ content: "Where?", file_ids: ["f1"] });
    expect(events.map((e) => e.type)).toEqual(["sources", "delta", "delta", "done"]);
    expect(
      events
        .filter((e) => e.type === "delta")
        .map((e) => e.data.text)
        .join(""),
    ).toBe("Fact [1].");
  });

  it("delivers a server error event and ends normally", async () => {
    const { events } = await collect(() =>
      sse([
        'event: delta\ndata: {"text":"Par"}\n\nevent: error\ndata: {"code":"llm_unavailable","message":"model down"}\n\n',
      ]),
    );
    expect(events.at(-1)).toEqual({
      type: "error",
      data: { code: "llm_unavailable", message: "model down" },
    });
  });

  it("turns HTTP errors into ApiErrors (rate limit)", async () => {
    const err = await collect(() =>
      apiError(429, "rate_limited", "too many questions, retry in 9s"),
    ).catch((e: unknown) => e);
    expect(err).toBeInstanceOf(ApiError);
    expect((err as ApiError).code).toBe("rate_limited");
  });

  it("reports a stream that ends early or breaks as interrupted", async () => {
    for (const response of [
      () => sse(['event: delta\ndata: {"text":"Par"}\n\n']),
      () => sse(['event: delta\ndata: {"text":"Par"}\n\n'], { error: true }),
    ]) {
      const err = await collect(response).catch((e: unknown) => e);
      expect((err as ApiError).code).toBe("stream_interrupted");
    }
  });

  it("rejects garbled event data", async () => {
    const err = await collect(() => sse(["event: delta\ndata: {not json\n\n"])).catch(
      (e: unknown) => e,
    );
    expect((err as ApiError).code).toBe("bad_stream");
  });

  it("stops when aborted", async () => {
    const controller = new AbortController();
    const { fetch } = fakeFetch({
      [PATH]: () =>
        sse([
          'event: delta\ndata: {"text":"a"}\n\n',
          'event: delta\ndata: {"text":"b"}\n\n',
          'event: delta\ndata: {"text":"c"}\n\n',
        ]),
    });
    const api = createApi({ baseUrl: BASE, fetch });
    const seen: string[] = [];
    const run = streamAnswer(
      api,
      ID,
      { content: "q" },
      (e) => {
        if (e.type === "delta") seen.push(e.data.text);
        controller.abort();
      },
      controller.signal,
    );
    await expect(run).rejects.toBeDefined();
    expect(seen.length).toBeLessThan(3);
  });
});

describe("decodeChatEvent", () => {
  it("ignores unknown events", () => {
    expect(decodeChatEvent({ event: "message", data: "hi" })).toBeNull();
  });
});
