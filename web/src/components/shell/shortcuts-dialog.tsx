import { Fragment } from "react";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Kbd } from "@/components/ui/kbd";

const MOD =
  typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.platform) ? "⌘" : "Ctrl";

const GROUPS: ReadonlyArray<{ title: string; items: ReadonlyArray<[string[][], string]> }> = [
  {
    title: "Anywhere",
    items: [
      [[[MOD, "K"]], "Open the command palette"],
      [[["/"]], "Search (focuses the search box on this screen)"],
      [[["U"]], "Upload files"],
      [[["?"]], "Show these shortcuts"],
      [[["Esc"]], "Close a dialog, menu or panel"],
    ],
  },
  {
    title: "Go to",
    items: [
      [[["G"], ["L"]], "Library"],
      [[["G"], ["S"]], "Search"],
      [[["G"], ["C"]], "Chat"],
      [[["G"], ["O"]], "Collections"],
      [[["G"], ["A"]], "Activity"],
      [[["G"], [","]], "Settings"],
    ],
  },
  {
    title: "Library",
    items: [
      [[["←"], ["→"], ["↑"], ["↓"]], "Move between files"],
      [[["Enter"]], "Open the file"],
      [[["Delete"]], "Delete the file (asks first)"],
    ],
  },
  {
    title: "Chat",
    items: [
      [[["Enter"]], "Send the question"],
      [[["Shift", "Enter"]], "New line"],
    ],
  },
];

/** The keyboard shortcuts sheet (`?`). */
export function ShortcutsDialog({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[80dvh] max-w-lg overflow-y-auto">
        <DialogHeader>
          <DialogTitle>Keyboard shortcuts</DialogTitle>
          <DialogDescription>Single keys work when you aren't typing in a field.</DialogDescription>
        </DialogHeader>
        <div className="grid gap-5">
          {GROUPS.map((group) => (
            <section key={group.title} aria-label={group.title} className="grid gap-2">
              <h3 className="eyebrow">{group.title}</h3>
              <dl className="grid gap-1.5">
                {group.items.map(([combos, label]) => (
                  <div key={label} className="flex items-center justify-between gap-4 text-sm">
                    <dt className="text-fg-muted">{label}</dt>
                    <dd className="flex shrink-0 items-center gap-1">
                      {combos.map((keys, i) => (
                        <Fragment key={keys.join("+")}>
                          {i > 0 ? (
                            <span className="text-2xs text-fg-subtle" aria-hidden>
                              {group.title === "Go to" ? "then" : "/"}
                            </span>
                          ) : null}
                          {keys.map((k) => (
                            <Kbd key={k}>{k}</Kbd>
                          ))}
                        </Fragment>
                      ))}
                    </dd>
                  </div>
                ))}
              </dl>
            </section>
          ))}
        </div>
      </DialogContent>
    </Dialog>
  );
}
