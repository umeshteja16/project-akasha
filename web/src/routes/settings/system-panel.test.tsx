import { createMemoryHistory } from "@tanstack/react-router";
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { App, createAppDeps } from "@/app";
import { fakeFetch, json, META, USER } from "@/test/fetch";

const STATUS = {
  version: "0.1.0",
  strict_offline: true,
  embedding: { name: "multilingual-e5-small", status: "ready", detail: null, dimensions: 384 },
  reranker: {
    name: "jina-reranker-v1-turbo-en",
    status: "unavailable",
    detail: "the model files are missing",
  },
  chat: { provider: "ollama", model: "llama3.1:8b", local: true, status: "ready" },
  onnx_runtime: { name: "libonnxruntime.so", status: "ready", detail: null },
  ocr: { name: "ocrs", status: "disabled", detail: null },
  relevance: { min_similarity: 0.8, min_rerank_score: -2 },
  worker: {
    health: "stalled",
    in_process: false,
    queued: 7,
    running: 0,
    retrying: 1,
    failed_last_day: 0,
    oldest_wait_secs: 300,
    last_finished_at: null,
  },
};

describe("settings → system", () => {
  it("shows models, privacy and the worker from the status endpoint", async () => {
    const { fetch } = fakeFetch({
      "GET /api/v1/meta": () => json(META),
      "GET /api/v1/me": () => json(USER),
      "GET /api/v1/system/status": () => json(STATUS),
    });
    const history = createMemoryHistory({ initialEntries: ["/settings?tab=system"] });
    render(<App deps={createAppDeps({ fetch, baseUrl: "http://akasha.test", history })} />);
    expect(await screen.findByText("multilingual-e5-small")).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "System", selected: true })).toBeInTheDocument();
    expect(screen.getByText("the model files are missing")).toBeInTheDocument();
    expect(screen.getByText(/runs on your machine or network/)).toBeInTheDocument();
    expect(screen.getByText(/only local language models are allowed/)).toBeInTheDocument();
    expect(screen.getByText("Stalled")).toBeInTheDocument();
    expect(screen.getByText(/7 waiting · 0 running · 1 retrying/)).toBeInTheDocument();
  });
});
