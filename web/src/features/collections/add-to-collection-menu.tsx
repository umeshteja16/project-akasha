import { useQuery } from "@tanstack/react-query";
import { PlusIcon } from "lucide-react";
import { type ReactNode, useState } from "react";
import { type Collection, collectionsQuery } from "@/api/collections";
import { useApi } from "@/api/context";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { toast } from "@/components/ui/toast";
import { sentence } from "@/lib/session";
import { CollectionFormDialog } from "./collection-form-dialog";
import { CollectionMark } from "./look";
import { useCollectionFiles } from "./mutations";

interface AddToCollectionMenuProps {
  fileIds: string[];
  /**
   * For a single file: the collections it is in. Items become checkboxes that
   * add or remove it; without this, items only add.
   */
  memberOf?: readonly string[];
  /** The trigger (a button), rendered `asChild`. */
  children: ReactNode;
  align?: "start" | "end";
}

function filesLabel(n: number): string {
  return n === 1 ? "1 file" : `${n} files`;
}

/** "Add to collection" for one or more files, with "New collection…". */
export function AddToCollectionMenu({
  fileIds,
  memberOf,
  children,
  align = "end",
}: AddToCollectionMenuProps) {
  const api = useApi();
  const collections = useQuery(collectionsQuery(api));
  const change = useCollectionFiles();
  const [creating, setCreating] = useState(false);
  const items = collections.data?.items ?? [];

  const toggle = (collection: Collection, remove: boolean) =>
    change.mutate(
      { id: collection.id, fileIds, remove },
      {
        onSuccess: (res) => {
          const n = res.file_ids.length;
          toast({
            title: remove
              ? `Removed from “${collection.name}”`
              : n === 0
                ? `Already in “${collection.name}”`
                : `Added ${filesLabel(n)} to “${collection.name}”`,
            tone: "success",
          });
        },
        onError: (e) => toast({ title: sentence(e.message), tone: "danger" }),
      },
    );

  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>{children}</DropdownMenuTrigger>
        <DropdownMenuContent align={align} className="max-h-80 w-64 overflow-y-auto">
          <DropdownMenuLabel className="eyebrow">
            {memberOf ? "Collections" : `Add ${filesLabel(fileIds.length)} to`}
          </DropdownMenuLabel>
          {collections.isPending ? (
            <p className="px-2.5 py-2 text-xs text-fg-subtle">Loading…</p>
          ) : items.length === 0 ? (
            <p className="px-2.5 py-2 text-xs text-fg-subtle">No collections yet.</p>
          ) : memberOf ? (
            items.map((c) => {
              const inIt = memberOf.includes(c.id);
              return (
                <DropdownMenuCheckboxItem
                  key={c.id}
                  checked={inIt}
                  onSelect={(e) => e.preventDefault()}
                  onCheckedChange={() => toggle(c, inIt)}
                >
                  <CollectionMark color={c.color} icon={c.icon} size="sm" />
                  <span className="flex-1 truncate">{c.name}</span>
                </DropdownMenuCheckboxItem>
              );
            })
          ) : (
            items.map((c) => (
              <DropdownMenuItem key={c.id} onSelect={() => toggle(c, false)}>
                <CollectionMark color={c.color} icon={c.icon} size="sm" />
                <span className="flex-1 truncate">{c.name}</span>
                <span className="font-mono text-2xs text-fg-subtle">{c.file_count}</span>
              </DropdownMenuItem>
            ))
          )}
          <DropdownMenuSeparator />
          <DropdownMenuItem onSelect={() => setCreating(true)}>
            <PlusIcon /> New collection…
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
      <CollectionFormDialog
        open={creating}
        onOpenChange={setCreating}
        fileIds={fileIds}
        onSaved={(c) =>
          toast({
            title: `Created “${c.name}” with ${filesLabel(c.file_count)}`,
            tone: "success",
          })
        }
      />
    </>
  );
}
