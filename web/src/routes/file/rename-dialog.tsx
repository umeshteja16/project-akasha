import { type FormEvent, useState } from "react";
import { isApiError } from "@/api/client";
import type { FileItem } from "@/api/files";
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
import { useUpdateFile } from "@/features/files/mutations";

export function RenameDialog({
  file,
  open,
  onOpenChange,
}: {
  file: FileItem;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const update = useUpdateFile();
  const [name, setName] = useState(file.name);
  const [error, setError] = useState<string | null>(null);

  const submit = (event: FormEvent) => {
    event.preventDefault();
    const next = name.trim();
    if (!next) {
      setError("Give it a name.");
      return;
    }
    if (next === file.name) {
      onOpenChange(false);
      return;
    }
    update.mutate(
      { id: file.id, changes: { name: next } },
      {
        onSuccess: () => onOpenChange(false),
        onError: (e) => setError(isApiError(e) ? e.message : "Couldn't rename it."),
      },
    );
  };

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (next) {
          setName(file.name);
          setError(null);
        }
        onOpenChange(next);
      }}
    >
      <DialogContent>
        <form onSubmit={submit} className="grid gap-4">
          <DialogHeader>
            <DialogTitle>Rename</DialogTitle>
            <DialogDescription>
              Only the name changes; the contents stay as they are.
            </DialogDescription>
          </DialogHeader>
          <Field
            label="Name"
            value={name}
            onChange={(e) => {
              setName(e.currentTarget.value);
              setError(null);
            }}
            error={error}
            autoFocus
            onFocus={(e) => {
              // Select the name without its extension.
              const dot = e.currentTarget.value.lastIndexOf(".");
              e.currentTarget.setSelectionRange(0, dot > 0 ? dot : e.currentTarget.value.length);
            }}
          />
          <DialogFooter>
            <Button variant="secondary" onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button type="submit" disabled={update.isPending}>
              {update.isPending ? "Saving…" : "Save name"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
