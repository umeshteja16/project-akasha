import { type KeyboardEvent, useRef, useState } from "react";
import type { FileItem } from "@/api/files";
import { cn } from "@/lib/utils";
import { type FileCardProps, FileRow, FileTile } from "./file-card";
import { columnsOf, isNavKey, moveIndex } from "./grid-navigation";
import type { LibraryView } from "./use-library-prefs";

interface FileCollectionProps {
  files: readonly FileItem[];
  view: LibraryView;
  selected: ReadonlySet<string>;
  selecting: boolean;
  onSelect: (id: string, selected: boolean) => void;
  onTogglePin: (file: FileItem) => void;
  onRetry: (file: FileItem) => void;
  /** Delete key: the selection, or the focused file when nothing is selected. */
  onDeleteRequest: (ids: string[]) => void;
  onClearSelection: () => void;
}

/**
 * The library's files as a grid of cards or a list of rows. One item is the tab
 * stop (roving focus); arrows, Home and End move between items, Space selects,
 * Enter opens, Delete asks to delete, Escape clears the selection.
 */
export function FileCollection({
  files,
  view,
  selected,
  selecting,
  onSelect,
  onTogglePin,
  onRetry,
  onDeleteRequest,
  onClearSelection,
}: FileCollectionProps) {
  const ref = useRef<HTMLUListElement>(null);
  const [activeIndex, setActiveIndex] = useState(0);
  const active = Math.min(activeIndex, Math.max(0, files.length - 1));

  const items = () => Array.from(ref.current?.querySelectorAll<HTMLElement>(":scope > li") ?? []);

  const focusIndex = (index: number) => {
    setActiveIndex(index);
    ref.current?.querySelector<HTMLElement>(`[data-nav-item="${index}"]`)?.focus();
  };

  const onKeyDown = (event: KeyboardEvent<HTMLUListElement>) => {
    const target = event.target as HTMLElement;
    const attr = target.getAttribute("data-nav-item");
    if (attr === null) return;
    const index = Number(attr);
    const file = files[index];
    if (!file) return;
    if (isNavKey(event.key)) {
      const columns = view === "grid" ? columnsOf(items()) : 1;
      const next = moveIndex(event.key, index, files.length, columns);
      event.preventDefault();
      if (next !== null) focusIndex(next);
      return;
    }
    if (event.key === " ") {
      event.preventDefault();
      onSelect(file.id, !selected.has(file.id));
    } else if (event.key === "Delete" || event.key === "Backspace") {
      event.preventDefault();
      onDeleteRequest(selected.size > 0 ? [...selected] : [file.id]);
    } else if (event.key === "Escape" && selected.size > 0) {
      event.preventDefault();
      onClearSelection();
    }
  };

  const cardProps = (file: FileItem, index: number): FileCardProps => ({
    file,
    index,
    active: index === active,
    selected: selected.has(file.id),
    selecting,
    onSelect,
    onTogglePin,
    onRetry,
    onFocusItem: setActiveIndex,
  });

  return (
    <ul
      ref={ref}
      aria-label="Files"
      aria-keyshortcuts="ArrowUp ArrowDown ArrowLeft ArrowRight Home End Space Delete"
      onKeyDown={onKeyDown}
      className={cn(
        view === "grid"
          ? "grid grid-cols-2 gap-3 sm:grid-cols-3 sm:gap-4 lg:grid-cols-4"
          : "divide-y divide-border overflow-hidden rounded-lg border border-border bg-surface",
      )}
    >
      {files.map((file, index) =>
        view === "grid" ? (
          <li key={file.id} className="min-w-0">
            <FileTile {...cardProps(file, index)} />
          </li>
        ) : (
          <li key={file.id}>
            <FileRow {...cardProps(file, index)} />
          </li>
        ),
      )}
    </ul>
  );
}
