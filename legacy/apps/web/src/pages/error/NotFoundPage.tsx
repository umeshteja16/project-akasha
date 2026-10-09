import { Link } from "react-router";
import { ArrowLeft } from "lucide-react";

export function NotFoundPage() {
  return (
    <div className="min-h-screen bg-bg-base text-text-primary flex flex-col justify-center items-center p-8 select-none font-sans relative overflow-hidden">
      {/* Editorial aesthetic context */}
      <div className="max-w-md text-center space-y-6 z-10">
        <span className="text-[11px] font-mono uppercase font-bold tracking-widest text-accent-primary">
          / error 404
        </span>
        <h1 className="font-serif text-6xl font-normal tracking-tight text-text-primary leading-none">
          Page not found.
        </h1>
        <p className="text-sm text-text-muted leading-relaxed font-medium">
          The requested coordinate inside the AKASHA index catalog could not be retrieved. It may have been relocated or does not exist.
        </p>
        <div className="pt-4">
          <Link
            to="/dashboard"
            className="inline-flex items-center gap-2 px-5 py-3 border border-border-default hover:border-border-strong rounded-xl text-xs font-semibold uppercase tracking-wider text-text-muted hover:text-text-primary transition-all bg-bg-surface hover:bg-bg-surface-raised cursor-pointer shadow-sm group"
          >
            <ArrowLeft className="h-4 w-4 transition-transform group-hover:-translate-x-1" />
            Return to Dashboard
          </Link>
        </div>
      </div>

      {/* Abstract structural overlay backdrop details */}
      <div className="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 pointer-events-none opacity-5 w-[800px] h-[800px] border border-dashed border-border-strong rounded-full select-none" />
      <div className="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 pointer-events-none opacity-5 w-[500px] h-[500px] border border-dashed border-border-strong rounded-full select-none" />
    </div>
  );
}
export default NotFoundPage;
