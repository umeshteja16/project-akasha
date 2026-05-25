import { Component, ErrorInfo, ReactNode } from "react";
import { AlertTriangle, RefreshCw } from "lucide-react";
import { NetworkErrorPage } from "../../pages/error/NetworkErrorPage";

interface Props {
  children: ReactNode;
  fallback?: ReactNode;
}

interface State {
  hasError: boolean;
  error: Error | null;
}

export class ErrorBoundary extends Component<Props, State> {
  public state: State = {
    hasError: false,
    error: null,
  };

  public static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error };
  }

  public componentDidCatch(error: Error, errorInfo: ErrorInfo) {
    console.error("ErrorBoundary caught an unhandled exception:", error, errorInfo);
  }

  private handleReset = () => {
    this.setState({ hasError: false, error: null });
    window.location.reload();
  };

  public render() {
    if (this.state.hasError) {
      if (this.props.fallback) {
        return this.props.fallback;
      }

      const errMsg = this.state.error?.message?.toLowerCase() || "";
      if (errMsg.includes("fetch") || errMsg.includes("network") || errMsg.includes("unreachable") || errMsg.includes("failed to connect")) {
        return (
          <NetworkErrorPage 
            error={this.state.error || undefined} 
            resetError={this.handleReset}
          />
        );
      }

      return (
        <div className="flex flex-col items-center justify-center p-8 border border-state-error/20 bg-bg-surface rounded-xl text-center max-w-lg mx-auto my-12 space-y-5 shadow-xl select-none">
          <div className="h-12 w-12 rounded-full bg-state-error/10 border border-state-error/25 flex items-center justify-center">
            <AlertTriangle className="h-6 w-6 text-state-error" />
          </div>
          <div className="space-y-2">
            <h3 className="font-serif text-lg text-text-primary">Something went wrong</h3>
            <p className="text-xs text-text-muted leading-relaxed max-w-sm mx-auto">
              An unexpected error occurred in this section of the application. The system remains secure, but this view had to be isolated.
            </p>
            {this.state.error?.message && (
              <pre className="p-3 bg-bg-base border border-border-default rounded-lg text-[11px] font-mono text-state-error/85 overflow-x-auto text-left max-w-full">
                {this.state.error.message}
              </pre>
            )}
          </div>
          <button
            onClick={this.handleReset}
            className="flex items-center gap-2 px-4 py-2 bg-accent-primary hover:bg-accent-hover text-text-primary rounded-lg text-xs font-semibold uppercase tracking-wider transition-all cursor-pointer shadow-md"
          >
            <RefreshCw className="h-3.5 w-3.5" />
            Reload Workspace
          </button>
        </div>
      );
    }

    return this.props.children;
  }
}
