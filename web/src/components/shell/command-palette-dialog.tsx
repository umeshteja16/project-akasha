import { useQuery } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { Command, defaultFilter } from "cmdk";
import {
  ArrowRightIcon,
  KeyboardIcon,
  LoaderIcon,
  LogOutIcon,
  MessageSquarePlusIcon,
  MonitorIcon,
  MoonIcon,
  SearchIcon,
  SunIcon,
  SwatchBookIcon,
  UploadIcon,
} from "lucide-react";
import { useCallback, useState } from "react";
import { useApi } from "@/api/context";
import { searchQuery } from "@/api/search";
import { Highlighted } from "@/components/common/highlighted";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { Kbd } from "@/components/ui/kbd";
import { kindOf } from "@/features/files/kind";
import { locationLabel, passageSearch } from "@/features/files/passage";
import { useUploader } from "@/features/upload/upload-context";
import { useTheme } from "@/lib/theme";
import { useDebouncedValue } from "@/lib/use-debounced";
import { isApple } from "@/lib/utils";
import { ALL_NAV_ITEMS } from "./nav-items";
import { openShortcuts } from "./shortcuts-store";
import { useSignOut } from "./use-sign-out";

const itemClass =
  "flex cursor-default items-center gap-3 rounded-md px-3 py-2 text-sm text-fg select-none data-[selected=true]:bg-surface-2 [&_svg]:size-4 [&_svg]:text-fg-subtle";
const groupClass =
  "[&_[cmdk-group-heading]]:eyebrow [&_[cmdk-group-heading]]:px-3 [&_[cmdk-group-heading]]:pt-3 [&_[cmdk-group-heading]]:pb-1.5";

export function CommandPalette({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const navigate = useNavigate();
  const { setPreference } = useTheme();
  const signOut = useSignOut();
  const { pick } = useUploader();
  const api = useApi();
  const [input, setInput] = useState("");
  const q = input.trim();
  const debounced = useDebouncedValue(q, 250);
  const live = debounced.length >= 2 && open;
  const results = useQuery({ ...searchQuery(api, { q: debounced }, 5), enabled: live });
  const hits = live && q.length >= 2 ? (results.data?.results ?? []) : [];

  const run = useCallback(
    (action: () => void) => {
      onOpenChange(false);
      action();
    },
    [onOpenChange],
  );

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) setInput("");
        onOpenChange(next);
      }}
    >
      <DialogContent hideClose className="max-w-xl gap-0 overflow-hidden p-0">
        <DialogTitle className="sr-only">Command palette</DialogTitle>
        <DialogDescription className="sr-only">
          Search your files, jump to a screen or run a command.
        </DialogDescription>
        <Command
          label="Command palette"
          className="flex flex-col"
          // Live search items always match (the server already ranked them).
          filter={(value, search, keywords) =>
            value.startsWith("live:") ? 1 : defaultFilter(value, search, keywords)
          }
        >
          <div className="flex items-center gap-3 border-b border-border px-4">
            <SearchIcon className="size-4 text-fg-subtle" aria-hidden />
            <Command.Input
              autoFocus
              value={input}
              onValueChange={setInput}
              placeholder="Search files or jump to…"
              className="h-13 flex-1 bg-transparent text-base text-fg outline-none placeholder:text-fg-subtle"
            />
            {live && results.isFetching ? (
              <LoaderIcon
                className="size-4 animate-spin text-fg-subtle motion-reduce:animate-none"
                aria-label="Searching"
              />
            ) : null}
            <Kbd>esc</Kbd>
          </div>
          <Command.List className="max-h-[min(60vh,24rem)] overflow-y-auto p-2">
            <Command.Empty className="px-3 py-8 text-center text-sm text-fg-muted">
              Nothing matches.
            </Command.Empty>
            {q ? (
              <Command.Group heading="Search" className={groupClass}>
                <Command.Item
                  value={`live:search ${q}`}
                  onSelect={() => run(() => void navigate({ to: "/search", search: { q } }))}
                  className={itemClass}
                >
                  <SearchIcon />
                  <span className="min-w-0 flex-1 truncate">
                    Search for <span className="font-medium">“{q}”</span>
                  </span>
                  <ArrowRightIcon />
                </Command.Item>
                {hits.map((hit) => {
                  const best = hit.matches[0];
                  const Icon = kindOf(hit.file.mime_type).icon;
                  return (
                    <Command.Item
                      key={hit.file.id}
                      value={`live:file ${hit.file.id}`}
                      onSelect={() =>
                        run(
                          () =>
                            void navigate({
                              to: "/files/$fileId",
                              params: { fileId: hit.file.id },
                              search: best
                                ? passageSearch(
                                    best.char_start,
                                    best.char_end,
                                    best.page,
                                    best.start_ms,
                                  )
                                : {},
                            }),
                        )
                      }
                      className={`${itemClass} items-start`}
                    >
                      <Icon className="mt-0.5" />
                      <span className="grid min-w-0 flex-1 gap-0.5">
                        <span className="truncate font-medium">{hit.file.name}</span>
                        {best ? (
                          <span className="line-clamp-1 text-xs text-fg-muted">
                            <Highlighted text={best.snippet.text} spans={best.snippet.highlights} />
                          </span>
                        ) : null}
                      </span>
                      {best && locationLabel(best) ? (
                        <span className="mt-0.5 font-mono text-2xs text-fg-subtle">
                          {locationLabel(best)}
                        </span>
                      ) : null}
                    </Command.Item>
                  );
                })}
              </Command.Group>
            ) : null}
            <Command.Group heading="Go to" className={groupClass}>
              {ALL_NAV_ITEMS.map((item) => (
                <Command.Item
                  key={item.to}
                  value={`go ${item.label}`}
                  onSelect={() => run(() => void navigate({ to: item.to }))}
                  className={itemClass}
                >
                  <item.icon />
                  {item.label}
                </Command.Item>
              ))}
              <Command.Item
                value="go design system"
                onSelect={() => run(() => void navigate({ to: "/design" }))}
                className={itemClass}
              >
                <SwatchBookIcon />
                Design system
              </Command.Item>
            </Command.Group>
            <Command.Group heading="Actions" className={groupClass}>
              <Command.Item value="upload files" onSelect={() => run(pick)} className={itemClass}>
                <UploadIcon /> Upload files
                <Kbd className="ml-auto">U</Kbd>
              </Command.Item>
              <Command.Item
                value="new chat ask a question"
                onSelect={() => run(() => void navigate({ to: "/chat", search: {} }))}
                className={itemClass}
              >
                <MessageSquarePlusIcon /> New chat
              </Command.Item>
              <Command.Item
                value="keyboard shortcuts help"
                onSelect={() => run(openShortcuts)}
                className={itemClass}
              >
                <KeyboardIcon /> Keyboard shortcuts
                <Kbd className="ml-auto">?</Kbd>
              </Command.Item>
            </Command.Group>
            <Command.Group heading="Theme" className={groupClass}>
              <Command.Item
                onSelect={() => run(() => setPreference("light"))}
                className={itemClass}
              >
                <SunIcon /> Light theme
              </Command.Item>
              <Command.Item onSelect={() => run(() => setPreference("dark"))} className={itemClass}>
                <MoonIcon /> Dark theme
              </Command.Item>
              <Command.Item
                onSelect={() => run(() => setPreference("system"))}
                className={itemClass}
              >
                <MonitorIcon /> Match system theme
              </Command.Item>
            </Command.Group>
            <Command.Group heading="Account" className={groupClass}>
              <Command.Item onSelect={() => run(() => signOut.mutate())} className={itemClass}>
                <LogOutIcon /> Sign out
              </Command.Item>
            </Command.Group>
          </Command.List>
          <div className="flex items-center justify-between border-t border-border bg-surface-2/60 px-4 py-2 text-xs text-fg-subtle">
            <span className="flex items-center gap-1.5">
              <Kbd>↑</Kbd>
              <Kbd>↓</Kbd> to move <Kbd>↵</Kbd> to open
            </span>
            <span className="flex items-center gap-1">
              <Kbd>{isApple() ? "⌘" : "Ctrl"}</Kbd>
              <Kbd>K</Kbd>
            </span>
          </div>
        </Command>
      </DialogContent>
    </Dialog>
  );
}
