import type { Collection } from "@/api/collections";
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
import { useDeleteCollection } from "@/features/collections/mutations";
import { sentence } from "@/lib/session";

/** "Delete the collection?" Its files stay in the library. */
export function DeleteCollectionDialog({
  collection,
  open,
  onOpenChange,
  onDeleted,
}: {
  collection: Collection;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onDeleted?: () => void;
}) {
  const remove = useDeleteCollection();
  const confirm = () =>
    remove.mutate(collection.id, {
      onSuccess: () => {
        onOpenChange(false);
        toast({ title: `Deleted “${collection.name}”`, tone: "success" });
        onDeleted?.();
      },
      onError: (e) => toast({ title: sentence(e.message), tone: "danger" }),
    });
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Delete “{collection.name}”?</DialogTitle>
          <DialogDescription>
            Only the collection goes. Its{" "}
            {collection.file_count === 1 ? "file stays" : "files stay"} in your library, searchable
            as before.
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button variant="secondary" onClick={() => onOpenChange(false)}>
            Keep it
          </Button>
          <Button variant="danger" onClick={confirm} disabled={remove.isPending}>
            {remove.isPending ? "Deleting…" : "Delete collection"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
