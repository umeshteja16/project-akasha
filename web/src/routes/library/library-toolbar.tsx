import { useQuery } from "@tanstack/react-query";
import {
  ArrowUpDownIcon,
  LayoutGridIcon,
  ListIcon,
  PinIcon,
  SparklesIcon,
  TagIcon,
  XIcon,
} from "lucide-react";
import { useApi } from "@/api/context";
import { type FileCategory, type FileSort, tagsQuery } from "@/api/files";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Segmented } from "@/components/ui/segmented";
import { CATEGORY_LABELS } from "@/features/files/kind";
import { cn } from "@/lib/utils";
import type { LibrarySearch } from "./library-search";
import type { LibraryView } from "./use-library-prefs";

export const SORT_LABELS: Record<FileSort, string> = {
  newest: "Newest first",
  oldest: "Oldest first",
  name: "Name, A to Z",
  size: "Largest first",
};

const TYPES: ReadonlyArray<FileCategory | "all"> = [
  "all",
  "pdf",
  "image",
  "text",
  "audio",
  "video",
];

interface LibraryToolbarProps {
  search: LibrarySearch;
  onSearchChange: (next: LibrarySearch) => void;
  sort: FileSort;
  onSortChange: (sort: FileSort) => void;
  view: LibraryView;
  onViewChange: (view: LibraryView) => void;
}

const chip =
  "inline-flex h-8 shrink-0 items-center gap-1.5 rounded-full border px-3 text-xs font-medium transition-colors [&_svg]:size-3.5";
const chipOff = "border-border bg-surface text-fg-muted hover:border-border-strong hover:text-fg";
const chipOn = "border-accent/40 bg-accent-soft text-accent-text";

export function LibraryToolbar({
  search,
  onSearchChange,
  sort,
  onSortChange,
  view,
  onViewChange,
}: LibraryToolbarProps) {
  const api = useApi();
  const tags = useQuery(tagsQuery(api));
  const tagItems = tags.data?.items ?? [];
  const set = (changes: Partial<LibrarySearch>) => {
    const next: LibrarySearch = { ...search, ...changes };
    for (const key of Object.keys(next) as (keyof LibrarySearch)[]) {
      if (next[key] === undefined) delete next[key];
    }
    onSearchChange(next);
  };

  return (
    <div className="flex flex-col gap-3 lg:flex-row lg:items-center lg:justify-between">
      <fieldset className="-mx-4 flex min-w-0 gap-1.5 overflow-x-auto px-4 pb-0.5 [scrollbar-width:none] sm:mx-0 sm:px-0">
        <legend className="sr-only">File type</legend>
        {TYPES.map((type) => {
          const on = type === "all" ? !search.type : search.type === type;
          return (
            <button
              key={type}
              type="button"
              aria-pressed={on}
              onClick={() => set({ type: type === "all" ? undefined : type })}
              className={cn(chip, on ? chipOn : chipOff)}
            >
              {type === "all" ? "All files" : CATEGORY_LABELS[type]}
            </button>
          );
        })}
      </fieldset>

      <div className="flex flex-wrap items-center gap-1.5">
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <button type="button" className={cn(chip, search.tag ? chipOn : chipOff)}>
              <TagIcon />
              {search.tag ? search.tag : "Tags"}
            </button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" className="max-h-80 w-60 overflow-y-auto">
            <DropdownMenuLabel className="eyebrow">Filter by tag</DropdownMenuLabel>
            {tagItems.length === 0 ? (
              <p className="px-2.5 py-2 text-xs text-fg-subtle">
                No tags yet. Add them on a file's page; suggested ones appear as files are read.
              </p>
            ) : (
              <DropdownMenuRadioGroup
                value={search.tag ?? ""}
                onValueChange={(tag) => set({ tag: tag || undefined })}
              >
                {tagItems.map((t) => (
                  <DropdownMenuRadioItem key={t.tag} value={t.tag}>
                    {t.user_files === 0 ? <SparklesIcon aria-label="suggested" /> : <TagIcon />}
                    <span className="flex-1 truncate">{t.tag}</span>
                    <span className="font-mono text-2xs text-fg-subtle">
                      {t.user_files + t.auto_files}
                    </span>
                  </DropdownMenuRadioItem>
                ))}
              </DropdownMenuRadioGroup>
            )}
            {search.tag ? (
              <>
                <DropdownMenuSeparator />
                <DropdownMenuItem onSelect={() => set({ tag: undefined })}>
                  <XIcon /> Clear tag filter
                </DropdownMenuItem>
              </>
            ) : null}
          </DropdownMenuContent>
        </DropdownMenu>

        <button
          type="button"
          aria-pressed={Boolean(search.pinned)}
          onClick={() => set({ pinned: search.pinned ? undefined : true })}
          className={cn(chip, search.pinned ? chipOn : chipOff)}
        >
          <PinIcon className={cn(search.pinned && "fill-current")} />
          Pinned
        </button>

        <span className="mx-1 hidden h-5 w-px bg-border sm:block" aria-hidden />

        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="ghost" size="sm" aria-label={`Sort: ${SORT_LABELS[sort]}`}>
              <ArrowUpDownIcon />
              <span className="hidden sm:inline">{SORT_LABELS[sort]}</span>
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuLabel className="eyebrow">Sort by</DropdownMenuLabel>
            <DropdownMenuRadioGroup value={sort} onValueChange={(v) => onSortChange(v as FileSort)}>
              {(Object.keys(SORT_LABELS) as FileSort[]).map((key) => (
                <DropdownMenuRadioItem key={key} value={key}>
                  {SORT_LABELS[key]}
                </DropdownMenuRadioItem>
              ))}
            </DropdownMenuRadioGroup>
          </DropdownMenuContent>
        </DropdownMenu>

        <Segmented
          label="View"
          value={view}
          onValueChange={onViewChange}
          options={[
            { value: "grid", label: "Grid", icon: <LayoutGridIcon /> },
            { value: "list", label: "List", icon: <ListIcon /> },
          ]}
        />
      </div>
    </div>
  );
}
