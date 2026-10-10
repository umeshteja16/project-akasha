import { useQuery } from "@tanstack/react-query";
import { SearchIcon } from "lucide-react";
import { useMemo, useState } from "react";
import { unwrap } from "@/api/client";
import type { Collection } from "@/api/collections";
import { useApi } from "@/api/context";
import { fileKeys } from "@/api/files";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { toast } from "@/components/ui/toast";
import { useCollectionFiles } from "@/features/collections/mutations";
import { FileThumb } from "@/features/files/file-thumb";
import { kindOf } from "@/features/files/kind";
import { formatRelative } from "@/lib/format";
import { sentence } from "@/lib/session";
import { cn } from "@/lib/utils";

/** Files offered (the newest); the filter narrows them by name. */
const OFFERED = 200;
const MAX_PICK = 100;

interface AddFilesDialogProps {
  collection: Collection;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

/** Pick files from the library to add to a collection. */
export function AddFilesDialog({ collection, open, onOpenChange }: AddFilesDialogProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="top-[8vh] max-w-xl">
        {open ? <Picker collection={collection} onDone={() => onOpenChange(false)} /> : null}
      </DialogContent>
    </Dialog>
  );
}

function Picker({ collection, onDone }: { collection: Collection; onDone: () => void }) {
  const api = useApi();
  const [filter, setFilter] = useState("");
  const [picked, setPicked] = useState<ReadonlySet<string>>(new Set());
  const change = useCollectionFiles();
  const list = (collectionId?: string) =>
    unwrap(
      api.GET("/api/v1/files", {
        params: {
          query: {
            limit: OFFERED,
            sort: "newest",
            ...(collectionId ? { collection_id: collectionId } : {}),
          },
        },
      }),
    );
  const library = useQuery({
    queryKey: [...fileKeys.lists(), "picker"],
    queryFn: () => list(),
  });
  const inside = useQuery({
    queryKey: [...fileKeys.lists(), "picker", collection.id],
    queryFn: () => list(collection.id),
  });
  const already = useMemo(
    () => new Set((inside.data?.items ?? []).map((f) => f.id)),
    [inside.data],
  );
  const needle = filter.trim().toLowerCase();
  const files = (library.data?.items ?? []).filter(
    (f) => !needle || f.name.toLowerCase().includes(needle),
  );

  const toggle = (id: string) =>
    setPicked((old) => {
      const next = new Set(old);
      if (next.has(id)) next.delete(id);
      else if (next.size < MAX_PICK) next.add(id);
      return next;
    });

  const add = () =>
    change.mutate(
      { id: collection.id, fileIds: [...picked] },
      {
        onSuccess: (res) => {
          const n = res.file_ids.length;
          toast({
            title: `Added ${n === 1 ? "1 file" : `${n} files`} to “${collection.name}”`,
            tone: "success",
          });
          onDone();
        },
        onError: (e) => toast({ title: sentence(e.message), tone: "danger" }),
      },
    );

  return (
    <>
      <DialogHeader>
        <DialogTitle>Add files to “{collection.name}”</DialogTitle>
        <DialogDescription>
          Choose from your library. Files stay where they are; a file can be in several collections.
        </DialogDescription>
      </DialogHeader>
      <div className="relative">
        <SearchIcon
          className="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-fg-subtle"
          aria-hidden
        />
        <Input
          type="search"
          value={filter}
          onChange={(e) => setFilter(e.currentTarget.value)}
          placeholder="Filter by name"
          aria-label="Filter files by name"
          className="pl-9"
        />
      </div>
      <fieldset className="-mx-2 max-h-[45vh] overflow-y-auto">
        <legend className="sr-only">Files</legend>
        {library.isPending ? (
          <p className="px-2 py-6 text-center text-sm text-fg-subtle">Loading your library…</p>
        ) : files.length === 0 ? (
          <p className="px-2 py-6 text-center text-sm text-fg-subtle">
            {needle ? "No file names match." : "Your library is empty."}
          </p>
        ) : (
          <ul className="grid gap-0.5">
            {files.map((f) => {
              const inIt = already.has(f.id);
              const on = inIt || picked.has(f.id);
              return (
                <li key={f.id}>
                  <label
                    className={cn(
                      "flex items-center gap-3 rounded-md px-2 py-1.5 transition-colors",
                      inIt ? "opacity-70" : "cursor-pointer hover:bg-surface-2",
                      picked.has(f.id) && "bg-accent-soft/60",
                    )}
                  >
                    <input
                      type="checkbox"
                      className="size-4 shrink-0 accent-[var(--accent)]"
                      checked={on}
                      disabled={inIt}
                      onChange={() => toggle(f.id)}
                    />
                    <FileThumb
                      id={f.id}
                      mime={f.mime_type}
                      status={f.status}
                      variant="icon"
                      className="size-9"
                    />
                    <span className="grid min-w-0 flex-1">
                      <span className="truncate text-sm text-fg">{f.name}</span>
                      <span className="text-xs text-fg-subtle">
                        {inIt
                          ? "Already in this collection"
                          : `${kindOf(f.mime_type).label} · added ${formatRelative(f.created_at)}`}
                      </span>
                    </span>
                  </label>
                </li>
              );
            })}
          </ul>
        )}
      </fieldset>
      <DialogFooter className="items-center">
        <p className="mr-auto text-xs text-fg-subtle" aria-live="polite">
          {picked.size === 0
            ? "Nothing chosen yet"
            : `${picked.size} chosen${picked.size === MAX_PICK ? " (the most at once)" : ""}`}
        </p>
        <Button variant="ghost" onClick={onDone}>
          Cancel
        </Button>
        <Button onClick={add} disabled={picked.size === 0 || change.isPending}>
          {change.isPending ? "Adding…" : picked.size > 1 ? `Add ${picked.size} files` : "Add"}
        </Button>
      </DialogFooter>
    </>
  );
}
