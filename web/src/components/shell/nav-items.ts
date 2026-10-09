import {
  LibraryBigIcon,
  type LucideIcon,
  MessageSquareTextIcon,
  SearchIcon,
  Settings2Icon,
} from "lucide-react";

export interface NavItem {
  to: "/library" | "/search" | "/chat" | "/settings";
  label: string;
  icon: LucideIcon;
  /** Single-key shortcut after `g` (g l, g s, ...). */
  key: string;
}

export const NAV_ITEMS: readonly NavItem[] = [
  { to: "/library", label: "Library", icon: LibraryBigIcon, key: "l" },
  { to: "/search", label: "Search", icon: SearchIcon, key: "s" },
  { to: "/chat", label: "Chat", icon: MessageSquareTextIcon, key: "c" },
  { to: "/settings", label: "Settings", icon: Settings2Icon, key: "," },
];
