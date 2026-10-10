import {
  FolderClosedIcon,
  HistoryIcon,
  LibraryBigIcon,
  type LucideIcon,
  MessageSquareTextIcon,
  SearchIcon,
  Settings2Icon,
} from "lucide-react";

export interface NavItem {
  to: "/library" | "/search" | "/chat" | "/settings" | "/collections" | "/activity";
  label: string;
  icon: LucideIcon;
  /** Single-key shortcut after `g` (g l, g s, ...). */
  key: string;
}

/** The four destinations of the phone tab bar (and the top of the sidebar). */
export const NAV_ITEMS: readonly NavItem[] = [
  { to: "/library", label: "Library", icon: LibraryBigIcon, key: "l" },
  { to: "/search", label: "Search", icon: SearchIcon, key: "s" },
  { to: "/chat", label: "Chat", icon: MessageSquareTextIcon, key: "c" },
  { to: "/settings", label: "Settings", icon: Settings2Icon, key: "," },
];

/** Further places: the sidebar, the phone's account menu and the palette. */
export const MORE_ITEMS: readonly NavItem[] = [
  { to: "/collections", label: "Collections", icon: FolderClosedIcon, key: "o" },
  { to: "/activity", label: "Activity", icon: HistoryIcon, key: "a" },
];

export const ALL_NAV_ITEMS: readonly NavItem[] = [...NAV_ITEMS, ...MORE_ITEMS];
