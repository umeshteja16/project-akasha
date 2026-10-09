import { CircleAlertIcon, LoaderIcon } from "lucide-react";
import type { FileStatus } from "@/api/files";
import { Badge } from "@/components/ui/badge";

const LABELS: Record<FileStatus, string> = {
  pending: "Queued",
  processing: "Reading",
  ready: "Ready",
  failed: "Failed",
};

/** Processing state; ready files show nothing unless `showReady`. */
export function StatusBadge({
  status,
  showReady = false,
}: {
  status: FileStatus;
  showReady?: boolean;
}) {
  if (status === "ready") return showReady ? <Badge tone="accent">Ready</Badge> : null;
  if (status === "failed") {
    return (
      <Badge tone="danger">
        <CircleAlertIcon className="size-3" aria-hidden />
        {LABELS.failed}
      </Badge>
    );
  }
  return (
    <Badge>
      <LoaderIcon className="size-3 animate-spin motion-reduce:animate-none" aria-hidden />
      {LABELS[status]}
    </Badge>
  );
}
