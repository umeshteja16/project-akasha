import { FileTextIcon, ImageIcon, LibraryBigIcon, NotebookPenIcon, UploadIcon } from "lucide-react";
import { EmptyState } from "@/components/common/empty-state";
import { PageHeader } from "@/components/common/page-header";
import { Button } from "@/components/ui/button";
import { toast } from "@/components/ui/toast";
import { Tooltip } from "@/components/ui/tooltip";

// How the library will read once it has files: a faded preview, not data.
const PREVIEW = [
  { icon: FileTextIcon, name: "Lease agreement, 2025.pdf", meta: "PDF · 14 pages", tag: "home" },
  {
    icon: ImageIcon,
    name: "Whiteboard, product sync.jpg",
    meta: "Image · text found",
    tag: "work",
  },
  {
    icon: NotebookPenIcon,
    name: "Reading notes — Middlemarch.md",
    meta: "Note · 2,140 words",
    tag: "books",
  },
] as const;

function comingSoon() {
  toast({
    title: "Uploads are almost here",
    description:
      "The upload flow lands in the next update. Dropping files already works as a preview.",
  });
}

export function LibraryPage() {
  return (
    <div className="grid gap-8">
      <PageHeader
        eyebrow="Your archive"
        title="Library"
        description="Documents, scans, images and notes you've kept. Akasha reads each one so you can find it by what it says."
        actions={
          <Tooltip content="Or drop files anywhere">
            <Button onClick={comingSoon}>
              <UploadIcon />
              Upload
            </Button>
          </Tooltip>
        }
      />
      <EmptyState icon={LibraryBigIcon} title="Nothing remembered yet">
        <p>
          Drop files anywhere on this page to begin. Text is extracted, scans are read, and every
          page becomes searchable.
        </p>
      </EmptyState>
      <section aria-label="Preview of a filled library" className="relative -mt-6">
        <ul className="divide-y divide-border overflow-hidden rounded-lg border border-border bg-surface opacity-60 select-none">
          {PREVIEW.map((row) => (
            <li key={row.name} className="flex items-center gap-4 px-4 py-3 sm:px-5">
              <row.icon className="size-4 shrink-0 text-fg-subtle" aria-hidden />
              <span className="min-w-0 flex-1 truncate text-sm text-fg">{row.name}</span>
              <span className="hidden text-xs text-fg-subtle sm:inline">{row.meta}</span>
              <span className="rounded-sm bg-surface-2 px-1.5 py-0.5 font-mono text-2xs text-fg-muted">
                {row.tag}
              </span>
            </li>
          ))}
        </ul>
        <div
          aria-hidden
          className="pointer-events-none absolute inset-0 bg-gradient-to-b from-transparent to-bg"
        />
      </section>
    </div>
  );
}
