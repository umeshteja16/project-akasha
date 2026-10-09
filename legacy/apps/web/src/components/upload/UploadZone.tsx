import { Upload, Loader2, CheckCircle2, AlertCircle } from "lucide-react";
import React, { useState } from "react";

interface UploadZoneProps {
  onUpload: (files: File[]) => void;
  isUploading: boolean;
  uploadProgress: number | null;
  uploadError: string | null;
  onClearError: () => void;
  acceptedFormats?: string;
  maxSizeMB?: number;
}

export function UploadZone({
  onUpload,
  isUploading,
  uploadProgress,
  uploadError,
  onClearError,
  acceptedFormats,
  maxSizeMB = 5000,
}: UploadZoneProps) {
  const [isDragOver, setIsDragOver] = useState(false);
  const [justCompleted, setJustCompleted] = useState(false);
  const [validationError, setValidationError] = useState<string | null>(null);

  const handleDragOver = (e: React.DragEvent) => {
    e.preventDefault();
    if (isUploading) return;
    setIsDragOver(true);
  };

  const handleDragLeave = () => {
    setIsDragOver(false);
  };

  const handleDrop = (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragOver(false);
    if (isUploading) return;
    setValidationError(null);

    if (e.dataTransfer.files && e.dataTransfer.files.length > 0) {
      const filesArray = Array.from(e.dataTransfer.files);
      const validFiles = filesArray.filter((file) => {
        if (file.size > maxSizeMB * 1024 * 1024) {
          setValidationError(`File ${file.name} is too large. Maximum size is ${maxSizeMB}MB.`);
          return false;
        }
        return true;
      });
      if (validFiles.length > 0) {
        onUpload(validFiles);
      }
    }
  };

  const handleFileChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    setValidationError(null);
    if (e.target.files && e.target.files.length > 0) {
      const filesArray = Array.from(e.target.files);
      const validFiles = filesArray.filter((file) => {
        if (file.size > maxSizeMB * 1024 * 1024) {
          setValidationError(`File ${file.name} is too large. Maximum size is ${maxSizeMB}MB.`);
          return false;
        }
        return true;
      });
      if (validFiles.length > 0) {
        onUpload(validFiles);
      }
    }
  };

  // Trigger brief completed flash
  React.useEffect(() => {
    let t: number;
    if (uploadProgress === 100) {
      setJustCompleted(true);
      t = window.setTimeout(() => setJustCompleted(false), 2000);
    }
    return () => {
      if (t) window.clearTimeout(t);
    };
  }, [uploadProgress]);

  // Automatically clear upload and validation errors after 10 seconds
  React.useEffect(() => {
    let t: number;
    if (uploadError || validationError) {
      t = window.setTimeout(() => {
        onClearError();
        setValidationError(null);
      }, 10000);
    }
    return () => {
      if (t) window.clearTimeout(t);
    };
  }, [uploadError, validationError, onClearError]);

  return (
    <div className="space-y-4 select-none">
      {/* Dashed outer drag zone */}
      <div
        onDragOver={handleDragOver}
        onDragLeave={handleDragLeave}
        onDrop={handleDrop}
        className={`w-full border-2 border-dashed rounded-xl py-12 px-6 flex flex-col items-center justify-center text-center transition-all duration-200 relative overflow-hidden ${
          isDragOver
            ? "border-accent-primary bg-accent-subtle bg-opacity-25"
            : isUploading
              ? "border-border-default bg-bg-surface/30 cursor-not-allowed"
              : justCompleted
                ? "border-state-success bg-state-success/5"
                : "border-border-strong bg-bg-surface/40 hover:border-border-strong hover:bg-bg-surface/60 cursor-pointer"
        }`}
      >
        <input
          type="file"
          id="drag-upload-input"
          className="hidden"
          accept={acceptedFormats}
          onChange={handleFileChange}
          disabled={isUploading}
          multiple
        />

        <label
          htmlFor={!isUploading ? "drag-upload-input" : undefined}
          className="w-full h-full flex flex-col items-center justify-center cursor-pointer absolute inset-0"
        />

        <div className="relative z-10 flex flex-col items-center justify-center space-y-4">
          {justCompleted ? (
            <div className="h-10 w-10 rounded-full bg-state-success/15 border border-state-success/30 flex items-center justify-center animate-scale text-state-success">
              <CheckCircle2 className="h-5 w-5" />
            </div>
          ) : isUploading ? (
            <div className="h-10 w-10 rounded-full bg-accent-subtle border border-accent-subtle-border flex items-center justify-center animate-pulse text-accent-primary">
              <Loader2 className="h-5 w-5 animate-spin" />
            </div>
          ) : (
            <div className={`h-10 w-10 rounded-full bg-bg-base border border-border-default flex items-center justify-center transition-colors ${isDragOver ? "border-accent-primary text-accent-primary" : "text-text-dim"}`}>
              <Upload className="h-5 w-5" />
            </div>
          )}

          <div className="space-y-1">
            <h4 className="text-sm font-semibold text-text-primary">
              {justCompleted
                ? "Uploaded successfully"
                : isDragOver
                   ? "Drop to upload"
                  : isUploading
                    ? "Uploading..."
                     : "Drop files here to get started"}
            </h4>
            <p className="text-xs text-text-muted">
              {isUploading
                ? `Uploading... ${uploadProgress || 0}%`
                : "Drag files here, or click to browse."}
            </p>
            <p className="text-[10px] text-text-dim uppercase tracking-wider pt-2.5">
              PDF · PNG · JPG · WEBP · TXT  ·  Max 50MB
            </p>
          </div>
        </div>

        {/* Ingesting progress bar layer overlay */}
        {isUploading && uploadProgress !== null && (
          <div className="absolute bottom-0 left-0 right-0 bg-bg-base/30 h-1">
            <div
              className="bg-accent-primary h-full transition-all duration-300 ease-out"
              style={{ width: `${uploadProgress}%` }}
            />
          </div>
        )}
      </div>

      {/* Embedded error banner */}
      {(uploadError || validationError) && (
        <div className="bg-state-error/10 border border-state-error/25 text-state-error text-xs rounded-xl p-4 flex items-start justify-between gap-3 animate-fade-in">
          <div className="flex items-start gap-3">
            <AlertCircle className="h-4.5 w-4.5 mt-0.5 shrink-0" />
            <div>
              <p className="font-semibold uppercase tracking-wide text-[10px]">Upload failed</p>
              <p className="leading-relaxed mt-0.5 select-text selection:bg-accent-primary/20">{uploadError || validationError}</p>
            </div>
          </div>
          <button
            onClick={() => {
              onClearError();
              setValidationError(null);
            }}
            className="text-[10px] uppercase font-bold text-text-dim hover:text-text-primary cursor-pointer shrink-0 transition-colors"
          >
            Clear
          </button>
        </div>
      )}
    </div>
  );
}
