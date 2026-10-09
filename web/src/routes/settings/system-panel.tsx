import { useQuery } from "@tanstack/react-query";
import { RefreshCwIcon } from "lucide-react";
import type { ReactNode } from "react";
import { useApi } from "@/api/context";
import { type ComponentStatus, type SystemStatus, systemStatusQuery } from "@/api/system";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { formatRelative } from "@/lib/format";
import { SettingsSection } from "./settings-section";

type Tone = "success" | "warning" | "danger" | "neutral";

const STATUS: Record<ComponentStatus, { label: string; tone: Tone }> = {
  ready: { label: "Ready", tone: "success" },
  not_loaded: { label: "Loads on first use", tone: "neutral" },
  downloads_on_first_use: { label: "Downloads on first use", tone: "neutral" },
  unavailable: { label: "Unavailable", tone: "danger" },
  disabled: { label: "Off", tone: "neutral" },
  not_needed: { label: "Not needed", tone: "neutral" },
};

function StatusBadge({ status }: { status: ComponentStatus }) {
  const { label, tone } = STATUS[status];
  return <Badge tone={tone}>{label}</Badge>;
}

/** One labelled line: what it is, its value, its state. */
function Row({
  label,
  value,
  status,
  detail,
}: {
  label: string;
  value: ReactNode;
  status?: ReactNode;
  detail?: string | null;
}) {
  return (
    <div className="grid gap-1 border-b border-border py-3 first:pt-0 last:border-b-0 last:pb-0 sm:grid-cols-[10rem_minmax(0,1fr)_auto] sm:items-baseline sm:gap-4">
      <dt className="text-sm text-fg-muted">{label}</dt>
      <dd className="min-w-0 text-sm break-words text-fg">
        {value}
        {detail ? <span className="mt-1 block text-xs text-danger">{detail}</span> : null}
      </dd>
      {status ? <dd className="sm:justify-self-end">{status}</dd> : null}
    </div>
  );
}

function Mono({ children }: { children: ReactNode }) {
  return <span className="font-mono text-xs">{children}</span>;
}

const HEALTH = {
  busy: { label: "Working", tone: "success" },
  idle: { label: "Idle", tone: "success" },
  stalled: { label: "Stalled", tone: "warning" },
} as const;

function Models({ s }: { s: SystemStatus }) {
  const chat =
    s.chat.provider === "none" ? (
      "None: chat answers with the passages themselves"
    ) : (
      <>
        <Mono>
          {s.chat.provider}
          {s.chat.model ? ` / ${s.chat.model}` : ""}
        </Mono>{" "}
        <span className="text-fg-subtle">
          · {s.chat.local ? "runs on your machine or network" : "cloud service"}
        </span>
      </>
    );
  return (
    <dl>
      <Row
        label="Embeddings"
        value={
          <>
            <Mono>{s.embedding.name}</Mono>{" "}
            <span className="text-fg-subtle">· {s.embedding.dimensions} dimensions</span>
          </>
        }
        status={<StatusBadge status={s.embedding.status} />}
        detail={s.embedding.detail}
      />
      <Row
        label="Reranker"
        value={s.reranker.name ? <Mono>{s.reranker.name}</Mono> : "Off"}
        status={<StatusBadge status={s.reranker.status} />}
        detail={s.reranker.detail}
      />
      <Row label="Chat model" value={chat} status={<StatusBadge status={s.chat.status} />} />
      <Row
        label="ONNX Runtime"
        value={s.onnx_runtime.name ? <Mono>{s.onnx_runtime.name}</Mono> : "—"}
        status={<StatusBadge status={s.onnx_runtime.status} />}
        detail={s.onnx_runtime.detail}
      />
      <Row
        label="Text recognition"
        value="OCR for images and scanned PDFs"
        status={<StatusBadge status={s.ocr.status} />}
        detail={s.ocr.detail}
      />
    </dl>
  );
}

function Privacy({ s }: { s: SystemStatus }) {
  const floor = [
    s.relevance.min_similarity != null ? `similarity ≥ ${s.relevance.min_similarity}` : null,
    s.relevance.min_rerank_score != null ? `rerank score ≥ ${s.relevance.min_rerank_score}` : null,
  ].filter(Boolean);
  return (
    <dl>
      <Row
        label="Strict offline"
        value={
          s.strict_offline
            ? "On: only local language models are allowed"
            : "Off: cloud language models may be configured"
        }
        status={
          <Badge tone={s.strict_offline ? "success" : "neutral"}>
            {s.strict_offline ? "On" : "Off"}
          </Badge>
        }
      />
      <Row
        label="Relevance floor"
        value={
          floor.length ? (
            <>
              <Mono>{floor.join(", ")}</Mono>
              <span className="mt-1 block text-xs text-fg-subtle">
                Results found by meaning alone below this are shown as "loosely related".
              </span>
            </>
          ) : (
            "None"
          )
        }
      />
    </dl>
  );
}

function Worker({ s }: { s: SystemStatus }) {
  const w = s.worker;
  const health = HEALTH[w.health];
  return (
    <dl>
      <Row
        label="Background worker"
        value={
          w.health === "stalled"
            ? "Jobs are waiting and nothing is taking them. Start a worker (akasha worker, or serve --with-worker)."
            : w.in_process
              ? "Runs inside this server"
              : "Runs as a separate process"
        }
        status={<Badge tone={health.tone}>{health.label}</Badge>}
      />
      <Row
        label="Queue"
        value={
          <span className="font-mono text-xs">
            {w.queued} waiting · {w.running} running · {w.retrying} retrying
            {w.failed_last_day ? ` · ${w.failed_last_day} failed today` : ""}
          </span>
        }
      />
      <Row
        label="Last job finished"
        value={w.last_finished_at ? formatRelative(w.last_finished_at) : "Not yet"}
      />
    </dl>
  );
}

/** Settings → System: which models and services this server runs with. */
export function SystemPanel() {
  const api = useApi();
  const status = useQuery(systemStatusQuery(api));
  const s = status.data;
  const body = (render: (s: SystemStatus) => ReactNode) =>
    s ? (
      render(s)
    ) : status.isError ? (
      <p className="text-sm text-danger" role="alert">
        Couldn't load the server status.
      </p>
    ) : (
      <div className="grid gap-3" role="status" aria-label="Loading">
        <Skeleton className="h-4 w-2/3" />
        <Skeleton className="h-4 w-1/2" />
        <Skeleton className="h-4 w-3/5" />
      </div>
    );

  return (
    <div className="grid">
      <SettingsSection
        id="models"
        title="Models"
        description="What reads, searches and answers. Models load on first use and run on this server."
      >
        <Card>
          <CardContent>
            {body((s) => (
              <Models s={s} />
            ))}
          </CardContent>
        </Card>
      </SettingsSection>
      <SettingsSection
        id="privacy"
        title="Privacy and search"
        description="Whether anything may leave this machine, and how strict search is."
      >
        <Card>
          <CardContent>
            {body((s) => (
              <Privacy s={s} />
            ))}
          </CardContent>
        </Card>
      </SettingsSection>
      <SettingsSection
        id="worker"
        title="Background work"
        description="Uploads are read, indexed and summarised by a background worker."
      >
        <Card>
          <CardContent>
            {body((s) => (
              <Worker s={s} />
            ))}
          </CardContent>
        </Card>
        <div className="mt-3 flex items-center justify-between gap-3 text-2xs text-fg-subtle">
          <span className="font-mono">{s ? `Akasha server ${s.version}` : null}</span>
          <Button
            variant="ghost"
            size="sm"
            onClick={() => void status.refetch()}
            disabled={status.isFetching}
          >
            <RefreshCwIcon
              className={status.isFetching ? "animate-spin motion-reduce:animate-none" : undefined}
              aria-hidden
            />
            Refresh
          </Button>
        </div>
      </SettingsSection>
    </div>
  );
}
