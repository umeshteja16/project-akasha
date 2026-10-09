import { isApiError } from "@/api/client";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { toast } from "@/components/ui/toast";
import { useDeleteFiles } from "./mutations";

interface ConfirmDeleteDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  ids: readonly string[];
  /** Name of the file when deleting just one. */
  name?: string;
  onDeleted?: (ids: string[]) => void;
}

/** "Delete forever?" for one or many files. */
export function ConfirmDeleteDialog({
  open,
  onOpenChange,
  ids,
  name,
  onDeleted,
}: ConfirmDeleteDialogProps) {
  const remove = useDeleteFiles();
  const count = ids.length;
  const what = count === 1 ? (name ? `“${name}”` : "this file") : `${count} files`;

  const confirm = () =>
    remove.mutate(ids, {
      onSuccess: (deleted) => {
        onOpenChange(false);
        toast({
          title: deleted.length === 1 ? "File deleted" : `${deleted.length} files deleted`,
          tone: "success",
        });
        onDeleted?.(deleted);
      },
    });

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) remove.reset();
        onOpenChange(next);
      }}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Delete {what}?</DialogTitle>
          <DialogDescription>
            {count === 1 ? "It is" : "They are"} removed from your library, search and chat sources.
            This can't be undone.
          </DialogDescription>
        </DialogHeader>
        {remove.error ? (
          <p role="alert" className="text-sm text-danger">
            {isApiError(remove.error) ? remove.error.message : "Couldn't delete. Try again."}
          </p>
        ) : null}
        <DialogFooter>
          <Button variant="secondary" onClick={() => onOpenChange(false)}>
            Keep {count === 1 ? "it" : "them"}
          </Button>
          <Button variant="danger" onClick={confirm} disabled={remove.isPending || count === 0}>
            {remove.isPending ? "Deleting…" : "Delete forever"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
