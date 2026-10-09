import type { ErrorComponentProps } from "@tanstack/react-router";
import { RotateCcwIcon } from "lucide-react";
import { isApiError } from "@/api/client";
import { Wordmark } from "@/components/common/wordmark";
import { Button } from "@/components/ui/button";
import { sentence } from "@/lib/session";
import { useDocumentTitle } from "@/lib/use-document-title";

/** Router error boundary: anything a route throws (loaders, render) ends up here. */
export function ErrorPage({ error, reset }: ErrorComponentProps) {
  useDocumentTitle("Something went wrong");
  const offline = isApiError(error) && error.status === 0;
  const detail = isApiError(error)
    ? sentence(error.message)
    : "An unexpected error stopped this page from loading.";
  return (
    <main role="alert" className="grid min-h-dvh place-items-center bg-bg px-6">
      <div className="grid max-w-md justify-items-center gap-4 text-center">
        <Wordmark />
        <h1 className="display mt-4 text-2xl text-fg">
          {offline ? "Can't reach Akasha" : "Something went wrong"}
        </h1>
        <p className="text-sm text-fg-muted">{detail}</p>
        <div className="mt-2 flex gap-2">
          <Button
            onClick={() => {
              reset();
              window.location.reload();
            }}
          >
            <RotateCcwIcon />
            Try again
          </Button>
          <Button asChild variant="secondary">
            <a href="/library">Go to library</a>
          </Button>
        </div>
        {import.meta.env.DEV && error instanceof Error ? (
          <pre className="mt-4 max-w-full overflow-auto rounded-md bg-surface-2 p-3 text-left font-mono text-xs text-fg-muted">
            {error.stack}
          </pre>
        ) : null}
      </div>
    </main>
  );
}
