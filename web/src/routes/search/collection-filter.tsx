import { useQuery } from "@tanstack/react-query";
import { FolderIcon } from "lucide-react";
import { collectionsQuery } from "@/api/collections";
import { useApi } from "@/api/context";
import { Chip } from "@/components/ui/chip";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { CollectionMark } from "@/features/collections/look";

/** "Search within" one collection (hidden until there are collections). */
export function CollectionFilter({
  value,
  onChange,
}: {
  value?: string;
  onChange: (collection?: string) => void;
}) {
  const api = useApi();
  const items = useQuery(collectionsQuery(api)).data?.items ?? [];
  const current = items.find((c) => c.id === value);
  if (items.length === 0 && !value) return null;
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Chip pressed={Boolean(value)}>
          <FolderIcon />
          <span className="max-w-40 truncate">
            {value ? (current?.name ?? "Collection") : "Collection"}
          </span>
        </Chip>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="max-h-80 w-60 overflow-y-auto">
        <DropdownMenuLabel className="eyebrow">Search within</DropdownMenuLabel>
        <DropdownMenuRadioGroup value={value ?? ""} onValueChange={(v) => onChange(v || undefined)}>
          <DropdownMenuRadioItem value="">All files</DropdownMenuRadioItem>
          {items.map((c) => (
            <DropdownMenuRadioItem key={c.id} value={c.id}>
              <CollectionMark color={c.color} icon={c.icon} size="sm" />
              <span className="flex-1 truncate">{c.name}</span>
            </DropdownMenuRadioItem>
          ))}
        </DropdownMenuRadioGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
