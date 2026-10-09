import { type ErrorComponentProps, Link, useRouter } from "@tanstack/react-router";
import { RotateCcwIcon, TriangleAlertIcon } from "lucide-react";
import { isApiError } from "@/api/client";
import { EmptyState } from "@/components/common/empty-state";
import { Button } from "@/components/ui/button";
import { sentence } from "@/lib/session";
import { useDocumentTitle } from "@/lib/use-document-title";

/** A newer build replaced the chunk this screen needs: only a reload helps. */
function isStaleChunk(error: unknown): boolean {
  return (
    error instanceof Error &&
    /dynamically imported module|Importing a module script failed|error loading dynamically/i.test(
      error.message,
    )
  );
}

/**
 * Error boundary for one screen: the shell (navigation) stays usable, only the
 * screen that failed is replaced.
 */
export function RouteError({ error, reset }: ErrorComponentProps) {
  const router = useRouter();
  useDocumentTitle("Something went wrong");
  const offline = isApiError(error) && error.status === 0;
  const stale = isStaleChunk(error);
  const detail = stale
    ? "Akasha was updated since this page was opened. Reload to get the new version."
    : offline
      ? "The server can't be reached. Check your connection, then try again."
      : isApiError(error)
        ? sentence(error.message)
        : "An unexpected error stopped this screen from loading. The rest of Akasha still works.";
  return (
    <div role="alert">
      <EmptyState
        icon={TriangleAlertIcon}
        title={offline ? "Can't reach Akasha" : "This screen ran into a problem"}
        actions={
          <>
            <Button
              onClick={() => {
                if (stale) {
                  window.location.reload();
                  return;
                }
                reset();
                void router.invalidate();
              }}
            >
              <RotateCcwIcon />
              {stale ? "Reload" : "Try again"}
            </Button>
            <Button asChild variant="secondary">
              <Link to="/library">Go to library</Link>
            </Button>
          </>
        }
      >
        <p>{detail}</p>
        {import.meta.env.DEV && error instanceof Error ? (
          <pre className="mt-4 max-w-full overflow-auto rounded-md bg-surface-2 p-3 text-left font-mono text-xs text-fg-muted">
            {error.stack}
          </pre>
        ) : null}
      </EmptyState>
    </div>
  );
}
