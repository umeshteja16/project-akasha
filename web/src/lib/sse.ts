// A minimal Server-Sent Events parser (the WHATWG event-stream format) for
// streamed `fetch` bodies. `EventSource` can only GET, and chat answers are POSTs.
//
// Chunks may split anywhere (inside a line, between "\r" and "\n", or inside a
// multi-byte character: the decoder handles that), and one chunk may carry many
// events. Comments (": keep-alive") and unknown fields are ignored.

export interface SseMessage {
  /** `event:` field; "message" when absent. */
  event: string;
  /** `data:` lines joined with "\n". */
  data: string;
  id?: string;
}

export class SseParser {
  private buffer = "";
  private event = "";
  private data: string[] = [];
  private id: string | undefined;
  private sawCr = false;

  constructor(private readonly onMessage: (message: SseMessage) => void) {}

  /** Feed the next piece of decoded text. */
  push(chunk: string): void {
    let text = chunk;
    // A "\r\n" split across chunks: the "\r" already ended the line.
    if (this.sawCr && text.startsWith("\n")) text = text.slice(1);
    this.sawCr = false;
    this.buffer += text;
    let start = 0;
    for (let i = 0; i < this.buffer.length; i++) {
      const c = this.buffer[i];
      if (c !== "\n" && c !== "\r") continue;
      this.line(this.buffer.slice(start, i));
      if (c === "\r") {
        if (i + 1 < this.buffer.length) {
          if (this.buffer[i + 1] === "\n") i++;
        } else {
          this.sawCr = true;
        }
      }
      start = i + 1;
    }
    this.buffer = this.buffer.slice(start);
  }

  /** The stream ended: an unterminated event is discarded, as the spec says. */
  end(): void {
    this.buffer = "";
    this.event = "";
    this.data = [];
  }

  private line(line: string): void {
    if (line === "") {
      this.dispatch();
      return;
    }
    if (line.startsWith(":")) return;
    const colon = line.indexOf(":");
    const field = colon === -1 ? line : line.slice(0, colon);
    let value = colon === -1 ? "" : line.slice(colon + 1);
    if (value.startsWith(" ")) value = value.slice(1);
    switch (field) {
      case "event":
        this.event = value;
        break;
      case "data":
        this.data.push(value);
        break;
      case "id":
        if (!value.includes("\0")) this.id = value;
        break;
      default:
        break;
    }
  }

  private dispatch(): void {
    if (this.data.length === 0) {
      this.event = "";
      return;
    }
    const message: SseMessage = { event: this.event || "message", data: this.data.join("\n") };
    if (this.id !== undefined) message.id = this.id;
    this.event = "";
    this.data = [];
    this.onMessage(message);
  }
}

/**
 * Read an event stream to its end, calling `onMessage` per event. Rejects with
 * the reader's error, or an `AbortError` once `signal` aborts (no events after it).
 */
export async function readSse(
  body: ReadableStream<Uint8Array>,
  onMessage: (message: SseMessage) => void,
  signal?: AbortSignal,
): Promise<void> {
  const parser = new SseParser((message) => {
    if (!signal?.aborted) onMessage(message);
  });
  const decoder = new TextDecoder();
  const reader = body.getReader();
  try {
    for (;;) {
      signal?.throwIfAborted();
      const { done, value } = await reader.read();
      if (done) break;
      parser.push(decoder.decode(value, { stream: true }));
    }
    signal?.throwIfAborted();
    parser.push(decoder.decode());
    parser.end();
  } catch (error) {
    void reader.cancel().catch(() => undefined);
    throw error;
  } finally {
    reader.releaseLock();
  }
}
