import { useQueryClient } from "@tanstack/react-query";
import { PlusIcon, SparklesIcon, XIcon } from "lucide-react";
import { type FormEvent, useState } from "react";
import { isApiError } from "@/api/client";
import type { FileItem } from "@/api/files";
import { replaceFile } from "@/features/files/cache";
import { type FileChanges, useUpdateFile } from "@/features/files/mutations";

const MAX_TAG = 50;

/** Normalise like the server: trimmed, lowercase. */
export function normalizeTag(raw: string): string {
  return raw.trim().toLowerCase();
}

/**
 * The person's tags (editable) and the model's suggestions, kept visibly apart:
 * a suggestion can be kept (moved into the person's tags) or dismissed.
 */
export function TagsEditor({ file }: { file: FileItem }) {
  const update = useUpdateFile();
  const [draft, setDraft] = useState("");
  const [error, setError] = useState<string | null>(null);

  const queryClient = useQueryClient();

  // Tags change on screen at once; a refused save puts them back.
  const save = (changes: FileChanges) => {
    setError(null);
    const before = file;
    replaceFile(queryClient, { ...file, ...changes });
    update.mutate(
      { id: file.id, changes },
      {
        onError: (e) => {
          replaceFile(queryClient, before);
          setError(isApiError(e) ? e.message : "Couldn't save the tags.");
        },
      },
    );
  };

  const add = (event: FormEvent) => {
    event.preventDefault();
    const tags = draft
      .split(",")
      .map(normalizeTag)
      .filter((t) => t && !file.tags.includes(t));
    if (tags.length === 0) {
      setDraft("");
      return;
    }
    if (tags.some((t) => t.length > MAX_TAG)) {
      setError(`Tags can be up to ${MAX_TAG} characters.`);
      return;
    }
    setDraft("");
    save({ tags: [...file.tags, ...new Set(tags)] });
  };

  return (
    <div className="grid gap-3">
      <ul className="flex flex-wrap gap-1.5" aria-label="Your tags">
        {file.tags.map((tag) => (
          <li
            key={tag}
            className="inline-flex h-7 items-center gap-1 rounded-full bg-surface-2 pr-1 pl-2.5 font-mono text-xs text-fg"
          >
            {tag}
            <button
              type="button"
              aria-label={`Remove tag ${tag}`}
              onClick={() => save({ tags: file.tags.filter((t) => t !== tag) })}
              className="grid size-6 place-items-center rounded-full text-fg-subtle hover:bg-surface-3 hover:text-fg"
            >
              <XIcon className="size-3" />
            </button>
          </li>
        ))}
        <li>
          <form onSubmit={add} className="flex">
            <label className="sr-only" htmlFor={`tag-input-${file.id}`}>
              Add a tag
            </label>
            <input
              id={`tag-input-${file.id}`}
              value={draft}
              onChange={(e) => setDraft(e.currentTarget.value)}
              placeholder={file.tags.length ? "Add tag" : "Add a tag…"}
              maxLength={MAX_TAG * 4}
              className="h-7 w-28 rounded-full border border-dashed border-border-strong/50 bg-transparent px-3 font-mono text-xs text-fg outline-none placeholder:text-fg-subtle focus:w-40 focus:border-accent focus:border-solid"
            />
          </form>
        </li>
      </ul>

      {file.auto_tags.length > 0 ? (
        <div className="grid gap-1.5">
          <p className="flex items-center gap-1.5 text-xs text-fg-subtle">
            <SparklesIcon className="size-3" aria-hidden /> Suggested from the contents
          </p>
          <ul className="flex flex-wrap gap-1.5" aria-label="Suggested tags">
            {file.auto_tags.map((tag) => (
              <li
                key={tag}
                className="inline-flex h-7 items-center rounded-full border border-dashed border-border-strong/50 font-mono text-xs text-fg-muted"
              >
                <button
                  type="button"
                  aria-label={`Keep suggested tag ${tag}`}
                  onClick={() => save({ tags: [...file.tags, tag] })}
                  className="inline-flex h-full items-center gap-1 rounded-l-full pr-1.5 pl-2.5 hover:bg-accent-soft hover:text-accent-text"
                >
                  <PlusIcon className="size-3" aria-hidden />
                  {tag}
                </button>
                <button
                  type="button"
                  aria-label={`Dismiss suggested tag ${tag}`}
                  onClick={() => save({ auto_tags: file.auto_tags.filter((t) => t !== tag) })}
                  className="grid h-full min-w-6 place-items-center rounded-r-full pr-1.5 pl-1 text-fg-subtle hover:bg-surface-2 hover:text-fg"
                >
                  <XIcon className="size-3" />
                </button>
              </li>
            ))}
          </ul>
        </div>
      ) : null}

      {error ? (
        <p role="alert" className="text-xs text-danger">
          {error}
        </p>
      ) : null}
    </div>
  );
}
