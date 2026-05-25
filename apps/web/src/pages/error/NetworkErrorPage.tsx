import { RefreshCw, WifiOff } from "lucide-react";

interface NetworkErrorPageProps {
  error?: Error;
  resetError?: () => void;
}

export function NetworkErrorPage({ error, resetError }: NetworkErrorPageProps) {
  const triggerReload = () => {
    if (resetError) {
      resetError();
    } else {
      window.location.reload();
    }
  };

  return (
    <div className="min-h-screen bg-bg-base text-text-primary flex flex-col justify-center items-center p-8 select-none font-sans relative overflow-hidden">
      <div className="max-w-md text-center space-y-6 z-10">
        <div className="flex justify-center">
          <div className="p-4 bg-state-error/10 border border-state-error/25 rounded-2xl text-state-error animate-pulse">
            <WifiOff className="h-8 w-8" />
          </div>
        </div>
        
        <span className="text-[11px] font-mono uppercase font-bold tracking-widest text-state-error block">
          / API CONNECTION OFFLINE
        </span>
        
        <h1 className="font-serif text-4xl font-normal tracking-tight text-text-primary leading-tight">
          Could not establish secure interface link.
        </h1>
        
        <p className="text-sm text-text-muted leading-relaxed font-medium">
          AKASHA was unable to connect to the backend server database engine. Verify that the Docker API service container is online and available locally.
        </p>

        {error && (
          <div className="bg-bg-surface border border-border-default rounded-xl p-3.5 text-left text-xs font-mono text-text-muted truncate">
            {error.message || String(error)}
          </div>
        )}

        <div className="pt-2">
          <button
            onClick={triggerReload}
            className="inline-flex items-center gap-2 px-5 py-3 border border-accent-subtle-border bg-accent-primary hover:bg-accent-hover text-bg-base rounded-xl text-xs font-semibold uppercase tracking-wider transition-all cursor-pointer shadow-sm group min-w-[150px] justify-center"
          >
            <RefreshCw className="h-4 w-4 transition-transform group-hover:rotate-45" />
            Retry Connection
          </button>
        </div>
      </div>

      <div className="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 pointer-events-none opacity-5 w-[800px] h-[800px] border border-dashed border-border-strong rounded-full select-none" />
    </div>
  );
}
export default NetworkErrorPage;
