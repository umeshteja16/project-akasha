import { Link } from "@tanstack/react-router";
import { FolderPlusIcon } from "lucide-react";
import type { FileDetail } from "@/api/files";
import { Button } from "@/components/ui/button";
import { AddToCollectionMenu } from "@/features/collections/add-to-collection-menu";
import { CollectionMark } from "@/features/collections/look";

/** The collections a file is in, and a menu to add it to (or take it out of) others. */
export function FileCollections({ file }: { file: FileDetail }) {
  const ids = file.collections.map((c) => c.id);
  return (
    <div className="grid gap-2.5">
      {file.collections.length ? (
        <ul className="grid gap-1" aria-label="In collections">
          {file.collections.map((c) => (
            <li key={c.id}>
              <Link
                to="/collections/$collectionId"
                params={{ collectionId: c.id }}
                className="-mx-1.5 flex items-center gap-2 rounded-md px-1.5 py-1 text-sm text-fg-muted transition-colors hover:bg-surface-2 hover:text-fg"
              >
                <CollectionMark color={c.color} icon={c.icon} size="sm" />
                <span className="truncate">{c.name}</span>
              </Link>
            </li>
          ))}
        </ul>
      ) : (
        <p className="text-xs text-fg-subtle">Not in any collection.</p>
      )}
      <div>
        <AddToCollectionMenu fileIds={[file.id]} memberOf={ids} align="start">
          <Button variant="secondary" size="sm">
            <FolderPlusIcon /> {ids.length ? "Change collections" : "Add to collection"}
          </Button>
        </AddToCollectionMenu>
      </div>
    </div>
  );
}
