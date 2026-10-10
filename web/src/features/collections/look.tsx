// A collection's look: its swatch (muted tint, never the accent) and icon.

import {
  ArchiveIcon,
  BookOpenIcon,
  BriefcaseIcon,
  Code2Icon,
  FlaskConicalIcon,
  FolderIcon,
  GraduationCapIcon,
  HeartIcon,
  HouseIcon,
  type LucideIcon,
  PlaneIcon,
  ReceiptTextIcon,
  StarIcon,
} from "lucide-react";
import type { CollectionColor, CollectionIcon } from "@/api/collections";
import { cn } from "@/lib/utils";

export const COLORS: Record<CollectionColor, { label: string; tile: string; dot: string }> = {
  sage: { label: "Sage", tile: "bg-swatch-sage-soft text-swatch-sage", dot: "bg-swatch-sage" },
  sky: { label: "Sky", tile: "bg-swatch-sky-soft text-swatch-sky", dot: "bg-swatch-sky" },
  ochre: { label: "Ochre", tile: "bg-swatch-ochre-soft text-swatch-ochre", dot: "bg-swatch-ochre" },
  clay: { label: "Clay", tile: "bg-swatch-clay-soft text-swatch-clay", dot: "bg-swatch-clay" },
  plum: { label: "Plum", tile: "bg-swatch-plum-soft text-swatch-plum", dot: "bg-swatch-plum" },
  slate: { label: "Slate", tile: "bg-swatch-slate-soft text-swatch-slate", dot: "bg-swatch-slate" },
};

export const ICONS: Record<CollectionIcon, { label: string; icon: LucideIcon }> = {
  folder: { label: "Folder", icon: FolderIcon },
  book: { label: "Book", icon: BookOpenIcon },
  briefcase: { label: "Work", icon: BriefcaseIcon },
  flask: { label: "Research", icon: FlaskConicalIcon },
  heart: { label: "Personal", icon: HeartIcon },
  star: { label: "Favourites", icon: StarIcon },
  archive: { label: "Archive", icon: ArchiveIcon },
  receipt: { label: "Receipts", icon: ReceiptTextIcon },
  plane: { label: "Travel", icon: PlaneIcon },
  home: { label: "Home", icon: HouseIcon },
  graduation: { label: "Study", icon: GraduationCapIcon },
  code: { label: "Code", icon: Code2Icon },
};

const SIZES = {
  sm: "size-6 rounded-sm [&_svg]:size-3.5",
  md: "size-9 rounded-md [&_svg]:size-[18px]",
  lg: "size-14 rounded-lg [&_svg]:size-7",
} as const;

/** The collection's icon on its tinted tile. Decorative: name it in text nearby. */
export function CollectionMark({
  color,
  icon,
  size = "md",
  className,
}: {
  color: CollectionColor;
  icon: CollectionIcon;
  size?: keyof typeof SIZES;
  className?: string;
}) {
  const Icon = ICONS[icon]?.icon ?? FolderIcon;
  return (
    <span
      aria-hidden
      className={cn(
        "grid shrink-0 place-items-center",
        COLORS[color]?.tile,
        SIZES[size],
        className,
      )}
    >
      <Icon strokeWidth={1.75} />
    </span>
  );
}
