import { CircleAlertIcon, CircleCheckIcon, InfoIcon, XIcon } from "lucide-react";
import { Toast as ToastPrimitive } from "radix-ui";
import { useSyncExternalStore } from "react";
import { cn } from "@/lib/utils";

export type ToastTone = "info" | "success" | "danger";

export interface ToastMessage {
  id: number;
  title: string;
  description?: string;
  tone: ToastTone;
}

// A tiny module-level store: `toast()` works from anywhere (mutations, the
// API client), not just components.
let toasts: ToastMessage[] = [];
let nextId = 1;
const listeners = new Set<() => void>();

function emit() {
  for (const listener of listeners) listener();
}

export function toast(input: { title: string; description?: string; tone?: ToastTone }): number {
  const id = nextId++;
  toasts = [...toasts.slice(-3), { id, tone: "info", ...input }];
  emit();
  return id;
}

export function dismissToast(id: number) {
  toasts = toasts.filter((t) => t.id !== id);
  emit();
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

const ICONS = { info: InfoIcon, success: CircleCheckIcon, danger: CircleAlertIcon } as const;

/** Mount once at the root. */
export function Toaster() {
  const items = useSyncExternalStore(
    subscribe,
    () => toasts,
    () => toasts,
  );
  return (
    <ToastPrimitive.Provider swipeDirection="right" duration={5000}>
      {items.map((item) => {
        const Icon = ICONS[item.tone];
        return (
          <ToastPrimitive.Root
            key={item.id}
            type={item.tone === "danger" ? "foreground" : "background"}
            onOpenChange={(open) => {
              if (!open) dismissToast(item.id);
            }}
            className={cn(
              "group relative flex w-full items-start gap-3 rounded-lg border border-border bg-surface p-4 pr-10 shadow-lg",
              "data-[state=open]:animate-rise-in data-[state=closed]:animate-fade-out",
              "data-[swipe=move]:translate-x-[var(--radix-toast-swipe-move-x)] data-[swipe=end]:animate-fade-out",
            )}
          >
            <Icon
              aria-hidden
              className={cn(
                "mt-0.5 size-4 shrink-0",
                item.tone === "success" && "text-accent-text",
                item.tone === "danger" && "text-danger",
                item.tone === "info" && "text-fg-subtle",
              )}
            />
            <div className="grid gap-0.5">
              <ToastPrimitive.Title className="text-sm font-medium text-fg">
                {item.title}
              </ToastPrimitive.Title>
              {item.description ? (
                <ToastPrimitive.Description className="text-sm text-fg-muted">
                  {item.description}
                </ToastPrimitive.Description>
              ) : null}
            </div>
            <ToastPrimitive.Close
              aria-label="Dismiss"
              className="absolute top-3 right-3 rounded-sm p-1 text-fg-subtle hover:bg-surface-2 hover:text-fg"
            >
              <XIcon className="size-3.5" />
            </ToastPrimitive.Close>
          </ToastPrimitive.Root>
        );
      })}
      <ToastPrimitive.Viewport className="fixed right-0 bottom-0 z-[100] flex w-full max-w-sm flex-col gap-2 p-4 pb-[calc(env(safe-area-inset-bottom)+5rem)] outline-none md:pb-4" />
    </ToastPrimitive.Provider>
  );
}
