// The processing story of one file, step by step.

import type { FileDetail } from "@/api/files";
import { formatRelative } from "@/lib/format";

export type StepState = "done" | "active" | "failed" | "waiting" | "skipped";

export interface Step {
  key: "upload" | "extract" | "embed" | "enrich";
  label: string;
  state: StepState;
  detail?: string;
  at?: string;
}

function jobDetail(file: FileDetail, running: string): string {
  const job = file.processing;
  if (!job) return "Waiting for a worker";
  switch (job.state) {
    case "queued":
      return job.attempts > 0
        ? `Queued again (attempt ${job.attempts + 1})`
        : "Waiting for a worker";
    case "running":
      return job.attempts > 1
        ? `${running} (attempt ${job.attempts} of ${job.max_attempts})`
        : running;
    case "failed": {
      const when = job.next_attempt_at ? ` ${formatRelative(job.next_attempt_at)}` : "";
      return `Attempt ${job.attempts} of ${job.max_attempts} failed, trying again${when}`;
    }
    default:
      return running;
  }
}

export function timelineSteps(file: FileDetail, chatModel: boolean, now = Date.now()): Step[] {
  const stage = file.processing?.stage ?? "extract";
  const ready = file.status === "ready";
  const failed = file.status === "failed";

  const extract: Step = { key: "extract", label: "Read the text", state: "waiting" };
  const embed: Step = { key: "embed", label: "Index for search", state: "waiting" };
  if (ready) {
    extract.state = "done";
    embed.state = "done";
    embed.at = file.processing?.updated_at;
  } else if (stage === "embed") {
    extract.state = "done";
    embed.state = failed ? "failed" : "active";
    embed.detail = failed
      ? (file.error ?? "Indexing failed")
      : jobDetail(file, "Computing embeddings");
  } else {
    extract.state = failed ? "failed" : "active";
    extract.detail = failed
      ? (file.error ?? "Extraction failed")
      : jobDetail(file, "Extracting text");
  }
  if (failed && stage !== "embed") embed.state = "skipped";

  const enrich: Step = { key: "enrich", label: "Summary and tags", state: "waiting" };
  const e = file.enrichment;
  if (e?.status === "done") {
    enrich.state = "done";
    enrich.at = e.updated_at ?? undefined;
    enrich.detail = e.model ? `Written by ${e.model}` : undefined;
  } else if (e?.status === "skipped") {
    enrich.state = "skipped";
    enrich.detail = "Nothing to describe";
  } else if (e?.status === "failed") {
    enrich.state = "failed";
    enrich.detail = "The model couldn't describe it";
  } else if (!chatModel) {
    enrich.state = "skipped";
    enrich.detail = "No language model configured";
  } else if (ready) {
    const recent = now - Date.parse(file.updated_at) < 90_000;
    enrich.state = recent ? "active" : "waiting";
    enrich.detail = recent ? "Writing a summary" : "Not described yet";
  } else if (failed) {
    enrich.state = "skipped";
  }

  return [
    { key: "upload", label: "Uploaded", state: "done", at: file.created_at },
    extract,
    embed,
    enrich,
  ];
}
