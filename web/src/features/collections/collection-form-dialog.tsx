import { CheckIcon } from "lucide-react";
import { type FormEvent, useId, useState } from "react";
import type { Collection, CollectionColor, CollectionIcon } from "@/api/collections";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Field } from "@/components/ui/field";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import { sentence } from "@/lib/session";
import { cn } from "@/lib/utils";
import { FormError } from "@/routes/auth/form-error";
import { COLORS, ICONS } from "./look";
import { type CollectionFields, useCreateCollection, useUpdateCollection } from "./mutations";

interface CollectionFormDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** Edit this collection; create a new one when absent. */
  collection?: Collection;
  /** Files to put into a new collection right away. */
  fileIds?: string[];
  onSaved?: (collection: Collection) => void;
}

/** Create or edit a collection: name, description, colour and icon. */
export function CollectionFormDialog(props: CollectionFormDialogProps) {
  return (
    <Dialog open={props.open} onOpenChange={props.onOpenChange}>
      <DialogContent className="max-w-lg">
        {/* Remounted per opening so the fields start from the collection. */}
        {props.open ? <CollectionForm {...props} /> : null}
      </DialogContent>
    </Dialog>
  );
}

function CollectionForm({ collection, fileIds, onOpenChange, onSaved }: CollectionFormDialogProps) {
  const [fields, setFields] = useState<CollectionFields>({
    name: collection?.name ?? "",
    description: collection?.description ?? "",
    color: collection?.color ?? "sage",
    icon: collection?.icon ?? "folder",
  });
  const create = useCreateCollection();
  const update = useUpdateCollection(collection?.id ?? "");
  const mutation = collection ? update : create;
  const descriptionId = useId();
  const set = (changes: Partial<CollectionFields>) => setFields((f) => ({ ...f, ...changes }));

  const submit = (event: FormEvent) => {
    event.preventDefault();
    const body = { ...fields, name: fields.name.trim(), description: fields.description.trim() };
    if (!body.name) return;
    const done = {
      onSuccess: (saved: Collection) => {
        onOpenChange(false);
        onSaved?.(saved);
      },
    };
    if (collection) update.mutate(body, done);
    else create.mutate({ ...body, ...(fileIds?.length ? { file_ids: fileIds } : {}) }, done);
  };

  return (
    <form onSubmit={submit} className="grid gap-5">
      <DialogHeader>
        <DialogTitle>{collection ? "Edit collection" : "New collection"}</DialogTitle>
        <DialogDescription>
          {collection
            ? "Rename it, describe it, or give it another look."
            : fileIds?.length
              ? `A named group for ${fileIds.length === 1 ? "this file" : `these ${fileIds.length} files`}. A file can be in several.`
              : "A named group of files, like “Taxes 2025” or “Thesis”. A file can be in several."}
        </DialogDescription>
      </DialogHeader>
      <FormError message={mutation.isError ? sentence(mutation.error.message) : null} />
      <Field
        label="Name"
        name="collection_name"
        autoComplete="off"
        maxLength={100}
        required
        placeholder="Taxes 2025"
        value={fields.name}
        onChange={(e) => set({ name: e.target.value })}
      />
      <div className="grid gap-1.5">
        <Label htmlFor={descriptionId}>
          Description <span className="font-normal text-fg-subtle">(optional)</span>
        </Label>
        <Textarea
          id={descriptionId}
          rows={2}
          maxLength={2000}
          placeholder="What belongs here"
          value={fields.description}
          onChange={(e) => set({ description: e.target.value })}
        />
      </div>
      <fieldset className="grid gap-2">
        <legend className="mb-2 text-sm font-medium text-fg">Colour</legend>
        <div className="flex flex-wrap gap-2">
          {(Object.keys(COLORS) as CollectionColor[]).map((color) => (
            <label
              key={color}
              className={cn(
                "relative grid size-9 cursor-pointer place-items-center rounded-full border-2 transition-colors",
                "has-[:focus-visible]:outline-2 has-[:focus-visible]:outline-offset-2 has-[:focus-visible]:outline-focus",
                fields.color === color
                  ? "border-fg"
                  : "border-transparent hover:border-border-strong",
              )}
            >
              <input
                type="radio"
                name="collection_color"
                value={color}
                checked={fields.color === color}
                onChange={() => set({ color })}
                className="sr-only"
              />
              <span className={cn("size-6 rounded-full", COLORS[color].dot)} aria-hidden />
              <span className="sr-only">{COLORS[color].label}</span>
            </label>
          ))}
        </div>
      </fieldset>
      <fieldset className="grid gap-2">
        <legend className="mb-2 text-sm font-medium text-fg">Icon</legend>
        <div className="grid grid-cols-6 gap-1.5">
          {(Object.keys(ICONS) as CollectionIcon[]).map((icon) => {
            const Icon = ICONS[icon].icon;
            const on = fields.icon === icon;
            return (
              <label
                key={icon}
                title={ICONS[icon].label}
                className={cn(
                  "relative grid h-10 cursor-pointer place-items-center rounded-md border transition-colors [&_svg]:size-[18px]",
                  "has-[:focus-visible]:outline-2 has-[:focus-visible]:outline-offset-2 has-[:focus-visible]:outline-focus",
                  on
                    ? cn("border-transparent", COLORS[fields.color].tile)
                    : "border-border text-fg-muted hover:border-border-strong hover:text-fg",
                )}
              >
                <input
                  type="radio"
                  name="collection_icon"
                  value={icon}
                  checked={on}
                  onChange={() => set({ icon })}
                  className="sr-only"
                />
                <Icon aria-hidden strokeWidth={1.75} />
                {on ? (
                  <CheckIcon
                    aria-hidden
                    className="absolute top-0.5 right-0.5 size-3! text-current"
                    strokeWidth={3}
                  />
                ) : null}
                <span className="sr-only">{ICONS[icon].label}</span>
              </label>
            );
          })}
        </div>
      </fieldset>
      <DialogFooter>
        <Button type="button" variant="ghost" onClick={() => onOpenChange(false)}>
          Cancel
        </Button>
        <Button type="submit" disabled={!fields.name.trim() || mutation.isPending}>
          {mutation.isPending ? "Saving…" : collection ? "Save" : "Create collection"}
        </Button>
      </DialogFooter>
    </form>
  );
}
