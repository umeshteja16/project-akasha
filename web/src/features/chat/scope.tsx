import { useQueries, useQuery } from "@tanstack/react-query";
import { FilesIcon, XIcon } from "lucide-react";
import { collectionQuery } from "@/api/collections";
import { useApi } from "@/api/context";
import { fileQuery } from "@/api/files";

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

/** `?files=` (comma-separated ids from the library's "Ask about these"). */
export function parseFileScope(raw: unknown): string[] {
  if (typeof raw !== "string") return [];
  const ids: string[] = [];
  for (const part of raw.split(",")) {
    const id = part.trim().toLowerCase();
    if (UUID.test(id) && !ids.includes(id)) ids.push(id);
  }
  return ids.slice(0, 100);
}

const SHOWN = 3;

/** "Answering from: a.pdf, b.txt +2" with a way back to the whole library. */
export function ScopeBar({ fileIds, onClear }: { fileIds: string[]; onClear: () => void }) {
  const api = useApi();
  const files = useQueries({
    queries: fileIds
      .slice(0, SHOWN)
      .map((id) => ({ ...fileQuery(api, id), refetchInterval: false as const })),
  });
  const names = files.map(
    (f, i) => f.data?.name ?? (f.isError ? "a deleted file" : `file ${i + 1}`),
  );
  const more = fileIds.length - names.length;
  return (
    <div className="flex items-center gap-2 rounded-lg bg-accent-soft/60 px-2.5 py-1.5 text-xs text-accent-text">
      <FilesIcon className="size-3.5 shrink-0" aria-hidden />
      <p className="min-w-0 flex-1 truncate">
        Answering only from <span className="font-medium">{names.join(", ")}</span>
        {more > 0 ? ` and ${more} more` : ""}
      </p>
      <button
        type="button"
        onClick={onClear}
        className="inline-flex shrink-0 items-center gap-1 rounded-sm px-1 font-medium hover:underline"
        aria-label="Ask the whole library instead"
      >
        <XIcon className="size-3.5" aria-hidden />
        <span className="hidden sm:inline">Whole library</span>
      </button>
    </div>
  );
}

/** "Answering from the collection Taxes 2025" with a way back to the whole library. */
export function CollectionScopeBar({
  collectionId,
  onClear,
}: {
  collectionId: string;
  onClear: () => void;
}) {
  const api = useApi();
  const collection = useQuery(collectionQuery(api, collectionId));
  const name = collection.data?.name ?? (collection.isError ? "a deleted collection" : "…");
  return (
    <div className="flex items-center gap-2 rounded-lg bg-accent-soft/60 px-2.5 py-1.5 text-xs text-accent-text">
      <FilesIcon className="size-3.5 shrink-0" aria-hidden />
      <p className="min-w-0 flex-1 truncate">
        Answering only from the collection <span className="font-medium">{name}</span>
      </p>
      <button
        type="button"
        onClick={onClear}
        className="inline-flex shrink-0 items-center gap-1 rounded-sm px-1 font-medium hover:underline"
        aria-label="Ask the whole library instead"
      >
        <XIcon className="size-3.5" aria-hidden />
        <span className="hidden sm:inline">Whole library</span>
      </button>
    </div>
  );
}
