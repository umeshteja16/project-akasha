import { useQuery } from "@tanstack/react-query";
import {
  CalendarIcon,
  PinIcon,
  SlidersHorizontalIcon,
  SparklesIcon,
  TagIcon,
  XIcon,
} from "lucide-react";
import { useId, useState } from "react";
import { useApi } from "@/api/context";
import type { FileCategory } from "@/api/files";
import { tagsQuery } from "@/api/files";
import type { SearchMode } from "@/api/search";
import { Button } from "@/components/ui/button";
import { Chip } from "@/components/ui/chip";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Popover, PopoverClose, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { Segmented } from "@/components/ui/segmented";
import { CATEGORY_LABELS } from "@/features/files/kind";
import {
  activeFilters,
  clearFilters,
  daysAgo,
  normalizeTags,
  type SearchParams,
  updateParams,
} from "@/features/search/search-params";
import { CollectionFilter } from "./collection-filter";

const TYPES: ReadonlyArray<FileCategory | "all"> = [
  "all",
  "pdf",
  "image",
  "text",
  "audio",
  "video",
];

export const MODE_HINTS: Record<SearchMode, { label: string; hint: string }> = {
  hybrid: { label: "Hybrid", hint: "Words and meaning together. Best for most searches." },
  keyword: { label: "Keyword", hint: "Only passages containing your words (or their forms)." },
  semantic: { label: "Semantic", hint: "Passages about the same idea, even in other words." },
};

const PRESETS = [
  { days: 7, label: "Past week" },
  { days: 30, label: "Past month" },
  { days: 365, label: "Past year" },
] as const;

const dayFormat = new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric" });

function dateLabel(from?: string, to?: string): string {
  const fmt = (d: string) => dayFormat.format(new Date(`${d}T00:00:00`));
  if (from && !to) {
    const preset = PRESETS.find((p) => daysAgo(p.days) === from);
    return preset ? preset.label : `Since ${fmt(from)}`;
  }
  if (from && to) return `${fmt(from)} – ${fmt(to)}`;
  if (to) return `Until ${fmt(to)}`;
  return "Any time";
}

interface SearchFiltersProps {
  params: SearchParams;
  onChange: (next: SearchParams) => void;
}

/** Type chips, tags, date and pinned; search mode tucked into "Advanced". */
export function SearchFilters({ params, onChange }: SearchFiltersProps) {
  const api = useApi();
  const tags = useQuery(tagsQuery(api)).data?.items ?? [];
  const selectedTags = params.tags ? normalizeTags(params.tags) : [];
  const [advanced, setAdvanced] = useState(Boolean(params.mode));
  const advancedId = useId();
  const set = (changes: Partial<SearchParams>) => onChange(updateParams(params, changes));
  const toggleTag = (tag: string) => {
    const next = selectedTags.includes(tag)
      ? selectedTags.filter((t) => t !== tag)
      : [...selectedTags, tag];
    set({ tags: next.length ? next.join(",") : undefined });
  };
  const mode: SearchMode = params.mode ?? "hybrid";

  return (
    <div className="grid gap-3">
      <div className="flex min-w-0 flex-col gap-2 lg:flex-row lg:items-center lg:justify-between">
        <fieldset className="-mx-4 flex min-w-0 gap-1.5 overflow-x-auto px-4 pb-0.5 [scrollbar-width:none] sm:mx-0 sm:px-0">
          <legend className="sr-only">File type</legend>
          {TYPES.map((type) => (
            <Chip
              key={type}
              pressed={type === "all" ? !params.type : params.type === type}
              onClick={() => set({ type: type === "all" ? undefined : type })}
            >
              {type === "all" ? "All files" : CATEGORY_LABELS[type]}
            </Chip>
          ))}
        </fieldset>

        <div className="flex flex-wrap items-center gap-1.5">
          <CollectionFilter
            value={params.collection}
            onChange={(collection) => set({ collection })}
          />
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Chip pressed={selectedTags.length > 0}>
                <TagIcon />
                {selectedTags.length === 0
                  ? "Tags"
                  : selectedTags.length === 1
                    ? selectedTags[0]
                    : `${selectedTags.length} tags`}
              </Chip>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="max-h-80 w-60 overflow-y-auto">
              <DropdownMenuLabel className="eyebrow">Files with all of</DropdownMenuLabel>
              {tags.length === 0 ? (
                <p className="px-2.5 py-2 text-xs text-fg-subtle">
                  No tags yet. Add them on a file's page.
                </p>
              ) : (
                tags.map((t) => (
                  <DropdownMenuCheckboxItem
                    key={t.tag}
                    checked={selectedTags.includes(t.tag)}
                    onSelect={(e) => e.preventDefault()}
                    onCheckedChange={() => toggleTag(t.tag)}
                  >
                    {t.user_files === 0 ? <SparklesIcon aria-label="suggested" /> : <TagIcon />}
                    <span className="flex-1 truncate">{t.tag}</span>
                  </DropdownMenuCheckboxItem>
                ))
              )}
              {selectedTags.length ? (
                <>
                  <DropdownMenuSeparator />
                  <DropdownMenuItem onSelect={() => set({ tags: undefined })}>
                    <XIcon /> Clear tags
                  </DropdownMenuItem>
                </>
              ) : null}
            </DropdownMenuContent>
          </DropdownMenu>

          <DateFilter params={params} onChange={(from, to) => set({ from, to })} />

          <Chip
            pressed={Boolean(params.pinned)}
            onClick={() => set({ pinned: params.pinned ? undefined : true })}
          >
            <PinIcon className={params.pinned ? "fill-current" : undefined} />
            Pinned
          </Chip>

          <span className="mx-1 hidden h-5 w-px bg-border sm:block" aria-hidden />

          <Button
            variant="ghost"
            size="sm"
            aria-expanded={advanced}
            aria-controls={advancedId}
            onClick={() => setAdvanced((v) => !v)}
          >
            <SlidersHorizontalIcon />
            Advanced
            {params.mode ? <span className="size-1.5 rounded-full bg-accent" aria-hidden /> : null}
          </Button>
        </div>
      </div>

      {advanced ? (
        <div
          id={advancedId}
          className="flex animate-fade-in flex-col gap-3 rounded-lg border border-border bg-surface-2/50 px-4 py-3 sm:flex-row sm:items-center"
        >
          <Segmented
            label="Search mode"
            value={mode}
            onValueChange={(m) => set({ mode: m === "hybrid" ? undefined : m })}
            options={(Object.keys(MODE_HINTS) as SearchMode[]).map((m) => ({
              value: m,
              label: MODE_HINTS[m].label,
            }))}
          />
          <p className="flex-1 text-xs text-fg-muted">
            {MODE_HINTS[mode].hint}{" "}
            <span className="text-fg-subtle">
              Use <code className="font-mono">"exact phrase"</code>,{" "}
              <code className="font-mono">or</code> and <code className="font-mono">-word</code>.
            </span>
          </p>
        </div>
      ) : null}

      {activeFilters(params) > 0 ? (
        <p className="text-xs text-fg-subtle">
          Filtered.{" "}
          <Button
            variant="link"
            size="sm"
            className="h-auto text-xs"
            onClick={() => onChange(clearFilters(params))}
          >
            Clear filters
          </Button>
        </p>
      ) : null}
    </div>
  );
}

function DateFilter({
  params,
  onChange,
}: {
  params: SearchParams;
  onChange: (from?: string, to?: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const [from, setFrom] = useState(params.from ?? "");
  const [to, setTo] = useState(params.to ?? "");
  const fromId = useId();
  const toId = useId();
  const on = Boolean(params.from || params.to);
  return (
    <Popover
      open={open}
      onOpenChange={(next) => {
        setOpen(next);
        if (next) {
          setFrom(params.from ?? "");
          setTo(params.to ?? "");
        }
      }}
    >
      <PopoverTrigger asChild>
        <Chip pressed={on}>
          <CalendarIcon />
          {dateLabel(params.from, params.to)}
        </Chip>
      </PopoverTrigger>
      <PopoverContent align="end" className="grid w-64 gap-3">
        <p className="eyebrow">Added</p>
        <div className="grid gap-1">
          {[{ days: 0, label: "Any time" }, ...PRESETS].map((preset) => {
            const value = preset.days ? daysAgo(preset.days) : undefined;
            const active = preset.days ? params.from === value && !params.to : !on;
            return (
              <PopoverClose asChild key={preset.label}>
                <button
                  type="button"
                  aria-pressed={active}
                  onClick={() => onChange(value, undefined)}
                  className="flex h-8 items-center justify-between rounded-sm px-2.5 text-left text-sm text-fg hover:bg-surface-2 aria-pressed:font-medium"
                >
                  {preset.label}
                  {active ? <span className="size-1.5 rounded-full bg-accent" aria-hidden /> : null}
                </button>
              </PopoverClose>
            );
          })}
        </div>
        <form
          className="grid gap-2 border-t border-border pt-3"
          onSubmit={(e) => {
            e.preventDefault();
            onChange(from || undefined, to || undefined);
            setOpen(false);
          }}
        >
          <div className="grid grid-cols-2 gap-2">
            <div className="grid gap-1">
              <Label htmlFor={fromId} className="text-xs">
                From
              </Label>
              <Input
                id={fromId}
                type="date"
                value={from}
                max={to || undefined}
                onChange={(e) => setFrom(e.currentTarget.value)}
                className="h-9 px-2 text-xs"
              />
            </div>
            <div className="grid gap-1">
              <Label htmlFor={toId} className="text-xs">
                To
              </Label>
              <Input
                id={toId}
                type="date"
                value={to}
                min={from || undefined}
                onChange={(e) => setTo(e.currentTarget.value)}
                className="h-9 px-2 text-xs"
              />
            </div>
          </div>
          <Button type="submit" size="sm" variant="secondary">
            Apply dates
          </Button>
        </form>
      </PopoverContent>
    </Popover>
  );
}
