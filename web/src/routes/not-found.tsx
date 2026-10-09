import { Link } from "@tanstack/react-router";
import { Wordmark } from "@/components/common/wordmark";
import { Button } from "@/components/ui/button";

export function NotFoundPage() {
  return (
    <div className="grid min-h-dvh place-items-center bg-bg px-6">
      <div className="grid max-w-md justify-items-center gap-5 text-center">
        <Wordmark />
        <p className="display text-[6rem] leading-none text-border-strong italic">404</p>
        <h1 className="display text-2xl text-fg">This page was never written down</h1>
        <p className="text-sm text-fg-muted">
          The address may be mistyped, or the page has moved. Your library is safe.
        </p>
        <Button asChild variant="secondary">
          <Link to="/library">Back to the library</Link>
        </Button>
      </div>
    </div>
  );
}
