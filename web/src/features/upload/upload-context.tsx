import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  createContext,
  type ReactNode,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
} from "react";
import { useApi } from "@/api/context";
import { metaQuery } from "@/api/queries";
import { addUploadedFile } from "@/features/files/cache";
import { type UploadItem, UploadQueue } from "./upload-queue";
import { type Uploader, xhrUploader } from "./xhr-upload";

interface UploadContextValue {
  queue: UploadQueue;
  /** Open the system file picker. */
  pick: () => void;
  add: (files: Iterable<File>) => void;
}

const UploadContext = createContext<UploadContextValue | null>(null);

export function useUploader(): UploadContextValue {
  const ctx = useContext(UploadContext);
  if (!ctx) throw new Error("useUploader must be used inside <UploadProvider>");
  return ctx;
}

export function useUploads(): readonly UploadItem[] {
  const { queue } = useUploader();
  return useSyncExternalStore(queue.subscribe, queue.getSnapshot, queue.getSnapshot);
}

/** Owns the upload queue and the hidden file input behind every Upload button. */
export function UploadProvider({
  children,
  uploader,
}: {
  children: ReactNode;
  uploader?: Uploader;
}) {
  const api = useApi();
  const queryClient = useQueryClient();
  const meta = useQuery(metaQuery(api));
  const maxBytes = useRef<number | null>(null);
  useEffect(() => {
    maxBytes.current = meta.data?.max_upload_bytes ?? null;
  }, [meta.data]);

  const [queue] = useState(
    () =>
      new UploadQueue({
        upload: uploader ?? xhrUploader(),
        maxBytes: () => maxBytes.current,
        onUploaded: ({ created, file }) => {
          if (created) addUploadedFile(queryClient, file);
        },
      }),
  );
  useEffect(() => () => queue.cancelAll(), [queue]);

  const input = useRef<HTMLInputElement>(null);
  const value = useMemo<UploadContextValue>(
    () => ({
      queue,
      pick: () => input.current?.click(),
      add: (files) => {
        queue.add(files);
      },
    }),
    [queue],
  );

  return (
    <UploadContext.Provider value={value}>
      {children}
      <input
        ref={input}
        type="file"
        multiple
        hidden
        data-testid="upload-input"
        aria-label="Choose files to upload"
        onChange={(event) => {
          const files = event.currentTarget.files;
          if (files && files.length > 0) queue.add(Array.from(files));
          event.currentTarget.value = "";
        }}
      />
    </UploadContext.Provider>
  );
}
