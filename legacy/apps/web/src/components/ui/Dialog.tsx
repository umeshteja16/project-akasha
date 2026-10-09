import { AlertTriangle, Loader2 } from "lucide-react";
import React from "react";

interface DialogProps {
  isOpen: boolean;
  title: string;
  description: string;
  cancelText?: string;
  confirmText?: string;
  onCancel: () => void;
  onConfirm: () => void;
  isConfirming?: boolean;
  variant?: "danger" | "info";
  children?: React.ReactNode;
}

export function Dialog({
  isOpen,
  title,
  description,
  cancelText = "Cancel",
  confirmText = "Confirm",
  onCancel,
  onConfirm,
  isConfirming = false,
  variant = "danger",
  children,
}: DialogProps) {
  if (!isOpen) return null;

  // Handle ESC key press to close
  React.useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        onCancel();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [onCancel]);

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 select-none">
      {/* Backdrop overlay with blur */}
      <div
        className="absolute inset-0 bg-black/65 backdrop-blur-sm transition-opacity"
        onClick={onCancel}
      />

      {/* Dialog card */}
      <div 
        role="dialog"
        aria-modal="true"
        aria-labelledby="dialog-title"
        className="relative bg-bg-surface border border-border-default hover:border-border-strong rounded-2xl max-w-md w-full p-6 shadow-2xl transition-all scale-100 flex flex-col gap-5"
      >
        <div className="flex items-start gap-4">
          <div
            className={`h-10 w-10 rounded-full flex items-center justify-center shrink-0 ${
              variant === "danger"
                ? "bg-state-error/10 border border-state-error/20"
                : "bg-accent-subtle border border-accent-subtle-border"
            }`}
          >
            <AlertTriangle
              className={`h-5 w-5 ${variant === "danger" ? "text-state-error" : "text-accent-primary"}`}
            />
          </div>
          <div className="space-y-1.5 min-w-0">
            <h3 id="dialog-title" className="font-serif text-base text-text-primary truncate">{title}</h3>
            <p className="text-xs text-text-muted leading-relaxed">{description}</p>
            {children}
          </div>
        </div>

        {/* Footer actions */}
        <div className="flex items-center justify-end gap-3 border-t border-border-default pt-4">
          <button
            onClick={onCancel}
            disabled={isConfirming}
            className="px-4 py-2 border border-border-default hover:border-border-strong rounded-lg text-xs font-semibold uppercase tracking-wider text-text-muted hover:text-text-primary transition-all disabled:opacity-50 cursor-pointer"
          >
            {cancelText}
          </button>
          <button
            onClick={onConfirm}
            disabled={isConfirming}
            className={`flex items-center gap-1.5 px-4 py-2 rounded-lg text-xs font-semibold uppercase tracking-wider transition-all disabled:opacity-50 cursor-pointer ${
              variant === "danger"
                ? "bg-state-error/15 border border-state-error/25 hover:bg-state-error/25 text-state-error"
                : "bg-accent-primary hover:bg-accent-hover text-text-primary"
            }`}
          >
            {isConfirming && <Loader2 className="h-3.5 w-3.5 animate-spin" />}
            {confirmText}
          </button>
        </div>
      </div>
    </div>
  );
}
