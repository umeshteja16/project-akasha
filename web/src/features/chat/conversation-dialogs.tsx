import { type FormEvent, useState } from "react";
import type { Conversation } from "@/api/chat";
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
import { Field } from "@/components/ui/field";
import { toast } from "@/components/ui/toast";
import { useDeleteConversation, useRenameConversation } from "./mutations";

export function conversationTitle(c: Pick<Conversation, "title"> | undefined): string {
  return c?.title.trim() || "New conversation";
}

interface DialogProps {
  conversation: Conversation | null;
  onOpenChange: (open: boolean) => void;
}

export function RenameConversationDialog({ conversation, onOpenChange }: DialogProps) {
  const rename = useRenameConversation();
  const [title, setTitle] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [forId, setForId] = useState<string | null>(null);
  if (conversation && conversation.id !== forId) {
    setForId(conversation.id);
    setTitle(conversation.title);
    setError(null);
  }

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (!conversation) return;
    const next = title.trim();
    if (!next) {
      setError("Give it a title.");
      return;
    }
    if (next.length > 200) {
      setError("Keep it under 200 characters.");
      return;
    }
    rename.mutate(
      { id: conversation.id, title: next },
      {
        onSuccess: () => onOpenChange(false),
        onError: (e) => setError(isApiError(e) ? e.message : "Couldn't rename it."),
      },
    );
  };

  return (
    <Dialog
      open={conversation !== null}
      onOpenChange={(open) => {
        if (!open) setForId(null);
        onOpenChange(open);
      }}
    >
      <DialogContent>
        <form onSubmit={submit} className="grid gap-4">
          <DialogHeader>
            <DialogTitle>Rename conversation</DialogTitle>
            <DialogDescription>A title you'll recognise in the list.</DialogDescription>
          </DialogHeader>
          <Field
            label="Title"
            value={title}
            maxLength={200}
            onChange={(e) => {
              setTitle(e.currentTarget.value);
              setError(null);
            }}
            error={error}
            autoFocus
            onFocus={(e) => e.currentTarget.select()}
          />
          <DialogFooter>
            <Button variant="secondary" onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button type="submit" disabled={rename.isPending}>
              Save title
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

export function DeleteConversationDialog({
  conversation,
  onOpenChange,
  onDeleted,
}: DialogProps & { onDeleted?: (id: string) => void }) {
  const remove = useDeleteConversation();
  return (
    <Dialog
      open={conversation !== null}
      onOpenChange={(open) => {
        if (!open) remove.reset();
        onOpenChange(open);
      }}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Delete “{conversationTitle(conversation ?? undefined)}”?</DialogTitle>
          <DialogDescription>
            The questions and answers are removed. Your files stay as they are. This can't be
            undone.
          </DialogDescription>
        </DialogHeader>
        {remove.isError ? (
          <p role="alert" className="text-sm text-danger">
            Couldn't delete it. Try again.
          </p>
        ) : null}
        <DialogFooter>
          <Button variant="secondary" onClick={() => onOpenChange(false)}>
            Keep it
          </Button>
          <Button
            variant="danger"
            disabled={remove.isPending}
            onClick={() => {
              if (!conversation) return;
              remove.mutate(conversation.id, {
                onSuccess: () => {
                  onOpenChange(false);
                  toast({ title: "Conversation deleted", tone: "success" });
                  onDeleted?.(conversation.id);
                },
              });
            }}
          >
            Delete forever
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
