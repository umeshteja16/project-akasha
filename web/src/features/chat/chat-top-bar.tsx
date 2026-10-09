import { PanelLeftIcon } from "lucide-react";
import { type ReactNode, useState } from "react";
import { Button } from "@/components/ui/button";
import { Sheet, SheetContent, SheetDescription, SheetTitle } from "@/components/ui/sheet";
import { ConversationList } from "./conversation-list";

/** The bar above a conversation; on phones it also opens the conversation list. */
export function ChatTopBar({ title, actions }: { title: ReactNode; actions?: ReactNode }) {
  const [open, setOpen] = useState(false);
  return (
    <div className="flex h-14 shrink-0 items-center gap-2 border-b border-border px-3 sm:px-5">
      <Button
        variant="ghost"
        size="icon-sm"
        className="md:hidden"
        aria-label="Conversations"
        onClick={() => setOpen(true)}
      >
        <PanelLeftIcon />
      </Button>
      <h1 className="display min-w-0 flex-1 truncate text-lg text-fg">{title}</h1>
      {actions}
      <Sheet open={open} onOpenChange={setOpen}>
        <SheetContent>
          <SheetTitle className="sr-only">Conversations</SheetTitle>
          <SheetDescription className="sr-only">
            Open, rename or delete a conversation.
          </SheetDescription>
          <ConversationList onNavigate={() => setOpen(false)} />
        </SheetContent>
      </Sheet>
    </div>
  );
}
