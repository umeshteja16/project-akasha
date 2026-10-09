import { Outlet } from "@tanstack/react-router";
import { Wordmark } from "@/components/common/wordmark";
import { ThemeToggle } from "@/components/shell/theme-toggle";

const PRINCIPLES = [
  ["Private", "Runs on your machine or server. Nothing leaves unless you choose a cloud model."],
  ["Searchable", "Words and meaning, across PDFs, scans, images and notes."],
  ["Grounded", "Answers quote the passages they come from, or say they don't know."],
] as const;

export function AuthLayout() {
  return (
    <div className="grid min-h-dvh bg-bg lg:grid-cols-[minmax(0,5fr)_minmax(0,6fr)]">
      <aside className="relative hidden flex-col justify-between overflow-hidden border-r border-border bg-sidebar p-12 lg:flex">
        <Wordmark size="lg" />
        <div className="max-w-md">
          <p className="eyebrow">A second memory</p>
          <blockquote className="display mt-4 text-4xl text-fg">
            A quiet place for everything you meant to remember.
          </blockquote>
          <dl className="mt-10 grid gap-5 border-t border-border pt-8">
            {PRINCIPLES.map(([term, detail], index) => (
              <div key={term} className="grid grid-cols-[2rem_1fr] gap-x-3">
                <span className="font-mono text-xs text-fg-subtle">0{index + 1}</span>
                <div>
                  <dt className="text-sm font-medium text-fg">{term}</dt>
                  <dd className="text-sm text-fg-muted">{detail}</dd>
                </div>
              </div>
            ))}
          </dl>
        </div>
        <p className="text-xs text-fg-subtle">Self-hosted · offline-first · yours</p>
        <span
          aria-hidden
          className="pointer-events-none absolute -right-24 -bottom-24 size-80 rounded-full border border-border"
        />
        <span
          aria-hidden
          className="pointer-events-none absolute -right-8 -bottom-8 size-48 rounded-full border border-border"
        />
      </aside>
      <main className="relative flex flex-col px-5 py-6 sm:px-10">
        <div className="flex items-center justify-between lg:justify-end">
          <span className="lg:hidden">
            <Wordmark />
          </span>
          <ThemeToggle />
        </div>
        <div className="flex flex-1 justify-center pt-10 pb-10 sm:items-center">
          <div className="w-full max-w-sm animate-rise-in">
            <p className="display mb-10 border-b border-border pb-8 text-2xl text-fg-muted italic lg:hidden">
              A quiet place for everything you meant to remember.
            </p>
            <Outlet />
          </div>
        </div>
      </main>
    </div>
  );
}
