# Project AKASHA - Sovereign Personal Knowledge Retrieval

Project AKASHA is a highly hardened, local-first personal knowledge retrieval platform designed to act as a private, sovereign semantic memory layer over a user's digital files. Instead of requiring manual document organization, the system understands content, extracts meaning, builds conceptual relationships, and enables natural language retrieval.

With AKASHA, users can upload PDFs, images, screenshots, notes, video files, and audio clips. The system extracts text, generates dense vector embeddings, and enables natural language queries.

---

## Core Product Modes

### Strict Sovereign Air-Gap Mode (STRICT_OFFLINE=true)
When enabled in the environment, the system completely bypasses external model API connections (e.g. Gemini), forcing the system to operate exclusively via the built-in offline local synthesis engine. A green "Strict Offline Mode" badge is active in the frontend navigation header.

### Hosted / Hybrid Mode
Standard self-hosted setup that uses the local containerized embedding/database pipeline, but calls external generative models (e.g., Gemini 2.5 Flash) via standard, zero-dependency REST queries to synthesize grounded multi-turn answers with inline citations.

---

## Architecture Overview

AKASHA is built with deep data sovereignty and offline-first principles at its core. It operates under a zero-trust model where files, chunks, embeddings, and cognitive reasoning do not leak to the public cloud.

```mermaid
graph TD
    UI[React Web App] -->|HTTPS REST| API[Fastify API Gateway]
    API -->|Ingest Signature Magic Bytes| API
    API -->|Job Queue| Redis[Redis 8 Queue]
    Redis -->|Worker Hook| Worker[Node Ingestion Worker]
    Worker -->|WASM OCR| OCR[Tesseract WASM]
    Worker -->|Embeddings API| EMB[Python FastAPI Embedding Service]
    EMB -->|Dense Vector all-mpnet-base-v2 768d| EMB
    Worker -->|Reranking API| RERANK[Python FastAPI Reranking Service]
    RERANK -->|Cross-Encoder ms-marco-MiniLM-L-6-v2| RERANK
    
    API -->|Cosine similarity pgvector HNSW| PG[(PostgreSQL 17 + pgvector + GIN FTS)]
    API -->|Strict Offline fallback / Grounded synthesis| RAG[Offline Synthesis / Gemini RAG turn]
    
    style UI fill:#1f1f2e,stroke:#6c63ff,stroke-width:2px,color:#fff
    style API fill:#1a233a,stroke:#3b82f6,stroke-width:2px,color:#fff
    style PG fill:#0f342e,stroke:#10b981,stroke-width:2px,color:#fff
    style EMB fill:#2c1b3f,stroke:#8b5cf6,stroke-width:2px,color:#fff
    style RAG fill:#3f1b2c,stroke:#f43f5e,stroke-width:2px,color:#fff
```

### System Boundaries
* **apps/web/**: React frontend. Owns UI rendering, routing, and user interaction. Makes HTTP calls to the API. No direct database or queue access.
* **apps/api/**: Fastify backend. Owns auth, file upload handling, metadata persistence, job dispatch, and search query handling. The single entry point for all client requests.
* **apps/workers/**: BullMQ worker processes. Own OCR, text extraction, embedding generation, and indexing. Consume jobs from Redis queues. Never called directly by the API - only through the queue.
* **services/embedding/**: Python FastAPI microservice. Owns embedding model loading and vector generation. Called by the embedding worker over HTTP. Stateless - no database access.
* **packages/db/**: Drizzle schema definitions and migrations. Shared by the API and workers. Single source of truth for the database model.
* **packages/shared/**: Shared TypeScript types, Zod schemas, and utility functions. No framework dependencies.

### Hardening and Security Invariants
* **Signature Security (Magic Bytes Check):** Uploaded content is scanned at the byte-header level (`verifyFileMagicBytes`). Masquerading executables or script payloads disguised with friendly extensions are blocked with HTTP `415` and instantly expunged.
* **Path Traversal Defense:** All filenames are strictly validated to prevent directory traversal sequences (`..`, `/`, `\`).
* **Content-Addressable CAS Storage:** Multi-tier duplicate file checking using SHA-256 content hashes guarantees physical storage deduplication. Safe reference-counting manages file links and ensures clean physical deletions when all pointers are severed.
* **Multi-Engine WASM OCR:** Document extraction is handled completely in-container using high-speed native PDF parsing and WebAssembly-compiled Tesseract OCR, ensuring 100% network-independent document processing.
* **Two-Stage Retrieval and Cross-Encoder Reranking:** Chunks retrieved through hybrid queries (60% Keyword FTS GIN score, 40% dense vector similarity) undergo a second-stage reranking via `ms-marco-MiniLM-L-6-v2`. This filters out context noise and isolates the top 5 high-precision candidates.
* **Hallucination Refusals:** Strict confidence score gates (`topScore < 0.35`) ensure the synthesis engine refuses to answer if no high-quality supporting evidence is located, enforcing total semantic truth.
* **Rootless Sandboxing:** Node.js and Python API containers execute under restricted, non-root users (`USER node` / `appuser`).
* **Capability Dropping:** Container privileges are systematically stripped (`cap_drop: [ALL]`), and privilege escalation is blocked (`no-new-privileges:true`) in production compose environments.
* **API Rate-Limiting:** Grounded chat routes `/chat/grounded` are restricted to 20 req/min and search routes `/search` to 30 req/min, blocking denial-of-service vector searches.
* **Dimension Alignment:** All dense vector columns in PostgreSQL (via pgvector) and query vectors must match exactly in size. The system uses the premium `all-mpnet-base-v2` dense model producing `768`-dimensional embeddings across `file_chunks.embedding` and `conversation_messages.query_embedding`.

---

## Technical Stack

| Layer | Technology | Role |
| :--- | :--- | :--- |
| Frontend | React + TypeScript + Vite | User interface, routing, server state management |
| Styling | Vanilla CSS / Tailored CSS | Sleek custom-styled dashboard, responsive layout |
| State | Zustand (UI) + TanStack Query (server) | UI state and async server data management |
| Forms | React Hook Form + Zod | Form handling and validation |
| Routing | React Router | Client-side navigation |
| Backend | Node.js + Fastify + TypeScript | REST API, authentication, upload handling, job dispatch |
| ORM | Drizzle ORM | Type-safe database queries and migrations |
| Validation | Zod | Request validation at API boundaries |
| Auth | JWT + bcrypt | Stateless auth with refresh token rotation |
| Database | PostgreSQL 17 | Metadata, users, file records, job state, GIN FTS |
| Vector Search | pgvector | Semantic embedding storage and cosine similarity search |
| Queue | BullMQ | Async job dispatch and worker orchestration |
| Cache | Redis 8 | Queue backend, search result cache |
| OCR | Tesseract OCR (WASM) | Image text extraction (WebAssembly compiled) |
| PDF Parsing | pdf-parse | PDF text extraction |
| Image Processing| sharp | Image preprocessing before OCR |
| Embedding Service| Python + FastAPI | Vector embedding generation microservice |
| Embedding Model| sentence-transformers (all-mpnet-base-v2)| 768-dimension premium semantic embeddings |
| Reranker Model | ms-marco-MiniLM-L-6-v2 | Cross-Encoder second-stage reranking (local) |
| File Storage | Local disk | Content-addressed storage (CAS) engine |
| Containerization| Docker + Docker Compose | Full-stack local developer environment orchestration |

---

## Storage Model and Core Database Schema

Structured metadata is persisted in PostgreSQL, while raw binaries are stored in content-addressed storage on disk.

```sql
-- Core schemas managed via Drizzle ORM

users (
  id uuid PRIMARY KEY,
  email varchar UNIQUE NOT NULL,
  password_hash varchar NOT NULL,
  created_at timestamp DEFAULT now()
);

files (
  id uuid PRIMARY KEY,
  owner_id uuid REFERENCES users(id),
  original_name varchar NOT NULL,
  storage_path varchar NOT NULL,
  content_hash varchar NOT NULL,
  mime_type varchar NOT NULL,
  size_bytes bigint NOT NULL,
  status varchar NOT NULL, -- pending, processing, completed, failed
  created_at timestamp DEFAULT now(),
  updated_at timestamp DEFAULT now()
);

extracted_content (
  id uuid PRIMARY KEY,
  file_id uuid REFERENCES files(id) ON DELETE CASCADE,
  raw_text text NOT NULL,
  ocr_version varchar NOT NULL,
  chunk_count integer NOT NULL,
  created_at timestamp DEFAULT now()
);

file_chunks (
  id uuid PRIMARY KEY,
  file_id uuid REFERENCES files(id) ON DELETE CASCADE,
  chunk_index integer NOT NULL,
  chunk_text text NOT NULL,
  embedding vector(768), -- premium dense semantic vector
  tsvector_content tsvector, -- for full-text search fallback
  created_at timestamp DEFAULT now()
);

conversation_messages (
  id uuid PRIMARY KEY,
  conversation_id uuid NOT NULL,
  role varchar NOT NULL, -- user, assistant
  content text NOT NULL,
  query_embedding vector(768), -- matched search vector
  created_at timestamp DEFAULT now()
);

processing_jobs (
  id uuid PRIMARY KEY,
  file_id uuid REFERENCES files(id) ON DELETE CASCADE,
  job_type varchar NOT NULL,
  status varchar NOT NULL,
  attempts integer DEFAULT 0,
  last_error text,
  created_at timestamp DEFAULT now()
);

audit_log (
  id uuid PRIMARY KEY,
  user_id uuid REFERENCES users(id),
  action varchar NOT NULL,
  file_id uuid,
  ip_address varchar,
  created_at timestamp DEFAULT now()
);
```

---

## Code Standards and Conventions

### General Rules
* Keep modules small and single-purpose. A file that does two things must be split.
* Fix root causes, not symptoms. Do not add workarounds on top of broken behavior.
* Export types from `packages/shared/` when they are used across more than one application or service.

### TypeScript
* Strict mode is required throughout the project (`"strict": true` in tsconfig).
* Never use `any`. Use explicit interfaces, `unknown` with narrowing, or generics.
* Validate all unknown external input at system boundaries using Zod.

### Fastify (Backend)
* Route handlers must validate and parse their inputs with a Zod schema before executing business logic.
* Enforce authentication middleware before ownership checks. Enforce ownership checks before any database mutation.
* Return consistent response shapes across all routes:
  - Success: `{ data: T }`
  - Error: `{ error: { code: string, message: string } }`
* Log all requests using Pino structured logging.

### React (Frontend)
* Default to functional components.
* Zustand manages UI-only state (e.g. sidebar open/closed, selected file, upload progress).
* TanStack Query manages all server state (e.g. file lists, search results, processing status polling). No direct data fetching in components using `useEffect`.
* Forms use React Hook Form with Zod resolvers.

### Queue and Workers
* Every BullMQ job payload is validated with a Zod schema at the start of the worker's `process` function.
* Workers must be idempotent. Re-running a job twice for the same file must produce the identical state.
* Workers catch all errors explicitly to prevent unhandled rejection crashes.

---

## Visual Theme and UX Specifications

AKASHA uses a sleek, dark-only editorial design language influenced by high-density infrastructural tools.

### Color Tokens
Component classes must use CSS custom property tokens defined in `:root`. No hardcoded hex values are allowed.

| Role | CSS Variable | Value |
| :--- | :--- | :--- |
| Page background | `--bg-base` | `#0a0b0f` |
| Surface (card) | `--bg-surface` | `#111318` |
| Surface elevated | `--bg-surface-raised`| `#181c22` |
| Surface hover | `--bg-surface-hover` | `#1e222a` |
| Border default | `--border-default` | `#1e2330` |
| Border strong | `--border-strong` | `#2a3040` |
| Primary text | `--text-primary` | `#e2e8f0` |
| Muted text | `--text-muted` | `#8892a4` |
| Placeholder / dim | `--text-dim` | `#4a5568` |
| Primary accent | `--accent-primary` | `#6c63ff` |
| Accent hover | `--accent-hover` | `#7d75ff` |
| Accent subtle bg | `--accent-subtle` | `rgba(108, 99, 255, 0.12)` |
| Accent subtle border| `--accent-subtle-border`| `rgba(108, 99, 255, 0.25)` |
| Success | `--state-success` | `#22c55e` |
| Warning | `--state-warning` | `#f59e0b` |
| Error | `--state-error` | `#ef4444` |
| Processing | `--state-processing` | `#3b82f6` |

### Typography
* **Display / Headings:** Serif font (Fraunces or DM Serif Display) - `--font-serif`. Used for hero titles and major section headings. Weight 400.
* **UI / Body / Labels:** Geist Sans - `--font-sans`. Used for all interface text below H2.
* **Code / Mono:** Geist Mono - `--font-mono`. Used for extracted text snippets, paths, and code blocks.

### Layout and Root Scaling
* **Root Density Scaling:** To prevent layout breakdowns and vertical overflow across different rendering engines (Chrome, Firefox, Safari), the root `html` font-size is locked to `14.4px` (90% of the standard 16px). This guarantees a high-density viewport layout that fits within full-screen height constraints (100vh) without requiring body-level scrollbars.
* **App Shell:** Fixed left sidebar (240px wide, `--bg-surface` background, `1px solid --border-default` right border), Top navbar (56px tall), and main content area (`--bg-base` background, padding 24px).

---

## Steps to Run and Setup

### 1. Prerequisites
Ensure you have the following installed on your host system:
* **Node.js:** version >= 22.0.0
* **pnpm:** version 10.0.0 (specified monorepo package manager)
* **Docker and Docker Compose:** For running database, Redis, and Python embedding microservices.

### 2. Environment Variables Configuration
Create a `.env` file in the root directory. You can use the following values as a baseline configuration:

```env
PORT=3001
NODE_ENV=development
DATABASE_URL=postgres://postgres:postgres@postgres:5432/akasha
REDIS_URL=redis://redis:6379
JWT_SECRET=supersecretjwtsigningkeyforakashamvp1234
UPLOAD_DIR=/storage/uploads
MAX_FILE_SIZE_MB=5000
STRICT_OFFLINE=false
```

If you plan to use external synthesis engines rather than strict local rules in hybrid mode, add your model API keys (e.g., `GEMINI_API_KEY`) to the `.env` file.

### 3. Run via Docker Compose (Recommended)
Docker Compose spins up the entire sovereign application architecture, including the python microservice and background workers, with dropped capabilities and non-root users.

To build and run all services:
```bash
# Build and spin up the containers
docker compose up --build
```

This starts the following grid:
* **akasha-postgres** at `localhost:5432` (using `pgvector:pg17` image)
* **akasha-redis** at `localhost:6379`
* **akasha-embedding** at `localhost:8000` (FastAPI sentence-transformer server)
* **akasha-api** at `localhost:3001` (Fastify REST Gateway)
* **akasha-worker** (BullMQ job consumer)
* **akasha-web** at `localhost:5173` (React Frontend client)

To stop the stack and keep volumes intact:
```bash
docker compose down
```

### 4. Running Database Migrations
Database schemas are managed using Drizzle ORM. You can run migrations directly from the host machine:

```bash
# Install host dependencies first
pnpm install

# Push the schema changes directly to the PostgreSQL instance
pnpm --filter @akasha/db db:push

# Alternatively, apply existing generated migrations
pnpm --filter @akasha/db db:migrate
```

### 5. Running in Host Development Mode (Hybrid Setup)
If you prefer to run Node.js and React services on your host machine for faster development reloading, you can leverage Docker for only the heavy services (PostgreSQL, Redis, and Python embedding):

```bash
# 1. Start database, cache, and embedding server in the background
docker compose up -d postgres redis embedding

# 2. Install dependencies on the host
pnpm install

# 3. Apply schema migrations
pnpm --filter @akasha/db db:push

# 4. Start all applications in development mode (launches API, workers, and web concurrently)
pnpm dev
```

---

## Verification and Testing

### Magic Bytes and Signature Testing
You can run automated tests checking security protections against masqueraded binary files and invalid mime types:
```bash
# Execute local magic bytes script
bash ./scratch/test_magic_bytes.sh
```

### Baseline Evaluations
To run baseline hybrid retrieval and Cross-Encoder reranking precision checks against the evaluation index:
```bash
# Run benchmark queries evaluator
node ./apps/api/retrieval_eval.js
```
