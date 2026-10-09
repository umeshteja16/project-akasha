# 0003: Run embeddings and reranking in-process

- Status: accepted · 2026-10-09

## Context
Legacy called a Python FastAPI service for embeddings and reranking. It blocked its event loop
and duplicated deployment.

## Decision
Run embedding and cross-encoder models inside the Rust binary via ONNX Runtime (`fastembed`).
Generative LLMs stay external behind a provider trait: Ollama (offline), Claude, Gemini.
"Strict offline" means only the Ollama provider is enabled.

## Consequences
No Python in production. The embedding model and its dimension are recorded in the database,
so changing models triggers a re-embed rather than silent mismatch.
