import { CheckIcon, CopyIcon, TriangleAlertIcon } from "lucide-react";
import { useState } from "react";
import { type CreatedToken, claudeCodeCommand, mcpEndpoint } from "@/api/tokens";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { toast } from "@/components/ui/toast";

function CopyBlock({ label, value }: { label: string; value: string }) {
  const [copied, setCopied] = useState(false);
  const copy = () => {
    void navigator.clipboard
      .writeText(value)
      .then(() => {
        setCopied(true);
        window.setTimeout(() => setCopied(false), 2000);
      })
      .catch(() => toast({ title: "Copy failed: select the text and copy it", tone: "danger" }));
  };
  return (
    <div className="grid gap-1.5">
      <div className="flex items-center justify-between gap-2">
        <span className="eyebrow">{label}</span>
        <Button type="button" variant="ghost" size="sm" onClick={copy} aria-label={`Copy ${label}`}>
          {copied ? <CheckIcon aria-hidden /> : <CopyIcon aria-hidden />}
          {copied ? "Copied" : "Copy"}
        </Button>
      </div>
      <code className="block rounded-md border border-border bg-surface-2 p-3 font-mono text-xs break-all text-fg select-all">
        {value}
      </code>
    </div>
  );
}

/** Shows a new token's secret, once, with ready-to-paste client setup. */
export function TokenCreatedDialog({
  created,
  onClose,
}: {
  created: CreatedToken | null;
  onClose: () => void;
}) {
  return (
    <Dialog open={created !== null} onOpenChange={(open) => (open ? undefined : onClose())}>
      <DialogContent className="max-w-xl">
        {created ? (
          <div className="grid gap-5">
            <DialogHeader>
              <DialogTitle>Copy your new token</DialogTitle>
              <DialogDescription>
                <span className="inline-flex items-center gap-1.5">
                  <TriangleAlertIcon className="size-4 text-warning" aria-hidden />
                  This is the only time it is shown. Store it somewhere safe.
                </span>
              </DialogDescription>
            </DialogHeader>
            <CopyBlock label="Token" value={created.secret} />
            <CopyBlock label="Claude Code" value={claudeCodeCommand(created.secret)} />
            <p className="text-sm text-fg-muted">
              Other MCP clients: connect to{" "}
              <code className="font-mono text-xs">{mcpEndpoint()}</code> with the header{" "}
              <code className="font-mono text-xs">Authorization: Bearer …</code>, or run{" "}
              <code className="font-mono text-xs">akasha mcp</code> with{" "}
              <code className="font-mono text-xs">AKASHA_TOKEN</code> for clients that start local
              programs. Use HTTPS when the server is reachable from the internet.
            </p>
            <DialogFooter>
              <DialogClose asChild>
                <Button>I've saved it</Button>
              </DialogClose>
            </DialogFooter>
          </div>
        ) : null}
      </DialogContent>
    </Dialog>
  );
}
