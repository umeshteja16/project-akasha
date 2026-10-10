import { useId, useState } from "react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import { useUploader } from "@/features/upload/upload-context";
import { noteFile } from "./note-file";

/** Jot a thought: saved as a Markdown file, then indexed like any upload. */
export function NewNoteDialog({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const id = useId();
  const { add } = useUploader();
  const [text, setText] = useState("");
  const save = () => {
    if (!text.trim()) return;
    add([noteFile(text)]);
    setText("");
    onOpenChange(false);
  };
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>New note</DialogTitle>
          <DialogDescription>
            Saved to your library as a Markdown file you can search and ask about.
          </DialogDescription>
        </DialogHeader>
        <Label htmlFor={id}>Note</Label>
        <Textarea
          id={id}
          value={text}
          rows={8}
          onChange={(event) => setText(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) save();
          }}
        />
        <DialogFooter>
          <Button variant="ghost" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button onClick={save} disabled={!text.trim()}>
            Save note
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
