import { UploadCloudIcon } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { toast } from "@/components/ui/toast";

function hasFiles(event: DragEvent): boolean {
  return Array.from(event.dataTransfer?.types ?? []).includes("Files");
}

/**
 * Drop files anywhere in the app. For now it only acknowledges the drop: the
 * upload flow (progress, duplicates, processing) is the next UI task.
 */
export function UploadDropZone() {
  const [active, setActive] = useState(false);
  const depth = useRef(0);

  useEffect(() => {
    const enter = (event: DragEvent) => {
      if (!hasFiles(event)) return;
      event.preventDefault();
      depth.current += 1;
      setActive(true);
    };
    const over = (event: DragEvent) => {
      if (hasFiles(event)) event.preventDefault();
    };
    const leave = (event: DragEvent) => {
      if (!hasFiles(event)) return;
      depth.current = Math.max(0, depth.current - 1);
      if (depth.current === 0) setActive(false);
    };
    const drop = (event: DragEvent) => {
      if (!hasFiles(event)) return;
      event.preventDefault();
      depth.current = 0;
      setActive(false);
      const count = event.dataTransfer?.files.length ?? 0;
      toast({
        title: count === 1 ? "1 file received" : `${count} files received`,
        description: "Uploading from here arrives with the next update of the library.",
      });
    };
    window.addEventListener("dragenter", enter);
    window.addEventListener("dragover", over);
    window.addEventListener("dragleave", leave);
    window.addEventListener("drop", drop);
    return () => {
      window.removeEventListener("dragenter", enter);
      window.removeEventListener("dragover", over);
      window.removeEventListener("dragleave", leave);
      window.removeEventListener("drop", drop);
    };
  }, []);

  if (!active) return null;
  return (
    <div
      aria-hidden
      className="pointer-events-none fixed inset-0 z-[60] grid animate-fade-in place-items-center bg-bg/80 p-6 backdrop-blur-sm"
    >
      <div className="flex w-full max-w-md flex-col items-center gap-3 rounded-xl border-2 border-dashed border-accent bg-surface/90 px-8 py-12 text-center shadow-lg">
        <UploadCloudIcon className="size-8 text-accent-text" strokeWidth={1.5} />
        <p className="display text-2xl text-fg">Drop to add to your library</p>
        <p className="text-sm text-fg-muted">PDFs, images, notes and text files.</p>
      </div>
    </div>
  );
}
