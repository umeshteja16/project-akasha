import { CanvasBackground } from "../../components/ui/CanvasBackground";
import { Link } from "react-router";
import { ArrowRight, Laptop, Cpu, Search, Sparkles } from "lucide-react";
import { useAuthStore } from "../../store/auth.store";

export function LandingPage() {
  const { accessToken } = useAuthStore();

  return (
    <div className="min-h-screen bg-bg-base flex flex-col justify-between relative overflow-hidden select-none">
      {/* Immersive star constellation background */}
      <CanvasBackground intensity="landing" />

      {/* Top Navbar */}
      <header className="h-20 flex items-center justify-between px-6 md:px-12 z-10 border-b border-border-default bg-bg-base/60 backdrop-blur-md">
        <Link to="/" className="flex items-center gap-3 hover:opacity-85 transition-opacity cursor-pointer">
          {/* Descending width Significa logomark */}
          <div className="flex flex-col gap-1 shrink-0">
            <div className="h-[2px] w-[22px] bg-text-primary"></div>
            <div className="h-[2px] w-[16px] bg-text-primary"></div>
            <div className="h-[2px] w-[10px] bg-text-primary"></div>
          </div>
          <span className="text-xs font-bold tracking-[0.2em] text-text-primary uppercase font-mono">
            AKASHA
          </span>
        </Link>

        <div className="flex items-center gap-6">
          {accessToken ? (
            <Link
              to="/dashboard"
              className="px-4 py-2 border border-text-primary bg-text-primary text-bg-base hover:bg-bg-base hover:text-text-primary text-[10px] font-bold uppercase tracking-widest transition-all cursor-pointer rounded-none animate-fade-in"
            >
              Dashboard
            </Link>
          ) : (
            <>
              <Link
                to="/login"
                className="text-[10px] font-bold uppercase tracking-widest text-text-muted hover:text-text-primary transition-all cursor-pointer animated-link-underline pb-0.5"
              >
                Sign In
              </Link>
              <Link
                to="/register"
                className="px-4 py-2 border border-text-primary bg-text-primary text-bg-base hover:bg-bg-base hover:text-text-primary text-[10px] font-bold uppercase tracking-widest transition-all cursor-pointer rounded-none"
              >
                Get Started
              </Link>
            </>
          )}
        </div>
      </header>

      {/* Main Hero Container */}
      <main className="flex-1 max-w-6xl mx-auto w-full px-6 md:px-12 py-12 md:py-24 grid grid-cols-1 lg:grid-cols-12 items-center gap-12 z-10">
        {/* Left Editorial Frame */}
        <div className="lg:col-span-7 space-y-8 text-left">
          <span className="inline-block px-3 py-1 border border-border-default text-[10px] font-bold uppercase tracking-widest text-text-muted font-mono">
            A Sovereign Knowledge Engine
          </span>
          <h1 className="font-serif text-7xl md:text-8xl font-normal tracking-tighter leading-none text-text-primary select-text selection:bg-accent-primary/20">
            Your collective<br />memory, unified.
          </h1>
          <p className="text-sm text-text-muted max-w-lg leading-relaxed font-sans font-medium select-text selection:bg-accent-primary/20">
            An offline-first, hyper-secure semantic database. Upload PDFs, notes, images, or media streams. AKASHA extracts, connects, and indexes your digital intellect—making it navigable via plain English queries with zero cloud dependency.
          </p>
          <div className="flex flex-wrap items-center gap-4 pt-2">
            <Link
              to={accessToken ? "/dashboard" : "/register"}
              className="flex items-center gap-2 px-5 py-3.5 border border-text-primary bg-text-primary text-bg-base hover:bg-bg-base hover:text-text-primary text-xs font-bold uppercase tracking-widest transition-all cursor-pointer rounded-none group"
            >
              {accessToken ? "Go to Dashboard" : "Get started"}
              <ArrowRight className="h-3.5 w-3.5 transition-transform group-hover:translate-x-1 shrink-0" />
            </Link>
            <button
              onClick={() => {
                document.getElementById("terminal-demo")?.scrollIntoView({ behavior: "smooth" });
              }}
              className="px-5 py-3.5 border border-border-default hover:border-border-strong text-text-muted hover:text-text-primary text-xs font-bold uppercase tracking-widest transition-all cursor-pointer bg-transparent rounded-none"
            >
              See it work
            </button>
          </div>
        </div>


        {/* Right Stark CLI Board */}
        <div id="terminal-demo" className="lg:col-span-5 w-full scroll-mt-24">
          <div className="relative group">
            {/* Monospace terminal board */}
            <div className="relative bg-bg-surface border border-border-strong rounded-none overflow-hidden shadow-2xl select-none">
              {/* Window dots */}
              <div className="px-4 py-3 bg-bg-base/60 border-b border-border-default flex items-center justify-between">
                <div className="flex items-center gap-1.5">
                  <span className="h-2 w-2 rounded-full bg-border-strong"></span>
                  <span className="h-2 w-2 rounded-full bg-border-strong"></span>
                  <span className="h-2 w-2 rounded-full bg-border-strong"></span>
                  <span className="text-[10px] font-mono text-text-dim ml-2">akasha — search preview</span>
                </div>
              </div>
              <div className="p-5 font-mono text-xs text-text-muted leading-relaxed space-y-4 select-text">
                <div>
                  <span className="text-text-primary">$</span> akasha search <span className="text-text-primary">"transformer attention mechanism"</span>
                </div>
                <div className="space-y-4 text-text-dim">
                  <div className="space-y-1">
                    <div className="flex items-center justify-between text-text-primary">
                      <span className="text-xs font-semibold">➜ deep-learning-notes.pdf</span>
                      <span className="border border-accent-subtle-border bg-accent-subtle text-text-primary px-2 py-0.5 text-[10px] font-bold">94% match</span>
                    </div>
                    <p className="pl-4 text-[11px] leading-relaxed text-text-muted select-text">
                      "The attention mechanism allows the model to weigh..."
                    </p>
                  </div>
                  <div className="h-px bg-border-default/45 my-2"></div>
                  <div className="space-y-1">
                    <div className="flex items-center justify-between text-xs text-text-primary">
                      <span className="font-semibold">➜ week-9-lecture-notes.png</span>
                      <span className="border border-border-default px-2 py-0.5 text-[10px] font-bold text-text-dim">87% match</span>
                    </div>
                    <p className="pl-4 text-[11px] leading-relaxed text-text-muted select-text">
                      "Self-attention computes queries, keys and values..."
                    </p>
                  </div>
                  <div className="h-px bg-border-default/45 my-2"></div>
                  <div className="space-y-1">
                    <div className="flex items-center justify-between text-xs text-text-primary">
                      <span className="font-semibold">➜ ml-paper-summary.txt</span>
                      <span className="border border-border-default px-2 py-0.5 text-[10px] font-bold text-text-dim">71% match</span>
                    </div>
                    <p className="pl-4 text-[11px] leading-relaxed text-text-muted select-text">
                      "Multi-head attention runs h attention functions..."
                    </p>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </div>
      </main>

      {/* How It Works Section */}
      <section className="border-t border-border-default bg-bg-base py-20 z-10 select-none">
        <div className="max-w-6xl mx-auto w-full px-6 md:px-12">
          <div className="grid grid-cols-1 md:grid-cols-3 divide-y md:divide-y-0 md:divide-x divide-border-default text-left">
            {/* Step 1 */}
            <div className="py-8 md:py-0 md:px-8 first:pl-0 last:pr-0 space-y-4">
              <span className="text-xs font-bold font-mono tracking-widest text-text-dim block">01/INGEST</span>
              <h4 className="text-lg font-semibold text-text-primary">Sovereign Ingestion</h4>
              <p className="text-xs text-text-muted leading-relaxed">
                Drag and drop documents, rich text files, audio/video streams, or images. AKASHA supports all major formats natively, completely secure on your own machine.
              </p>
            </div>
            {/* Step 2 */}
            <div className="py-8 md:py-0 md:px-8 first:pl-0 last:pr-0 space-y-4">
              <span className="text-xs font-bold font-mono tracking-widest text-text-dim block">02/SYNTHESIZE</span>
              <h4 className="text-lg font-semibold text-text-primary">Semantic Indexing</h4>
              <p className="text-xs text-text-muted leading-relaxed">
                Our local engine automatically extracts text, chunks files dynamically, and generates dense vector embeddings asynchronously without blocking your workflow.
              </p>
            </div>
            {/* Step 3 */}
            <div className="py-8 md:py-0 md:px-8 first:pl-0 last:pr-0 space-y-4">
              <span className="text-xs font-bold font-mono tracking-widest text-text-dim block">03/NAVIGATE</span>
              <h4 className="text-lg font-semibold text-text-primary">Intelligent Recall</h4>
              <p className="text-xs text-text-muted leading-relaxed">
                Query your entire vault in plain English. Instantly retrieve exact document excerpts coupled with bulletproof inline citations.
              </p>
            </div>
          </div>
        </div>
      </section>

      {/* Feature grid */}
      <section className="border-t border-border-default bg-bg-base py-20 z-10 select-none">
        <div className="max-w-6xl mx-auto w-full px-6 md:px-12 grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-12 text-left">
          {/* Card 1 */}
          <div className="space-y-4 border-t border-border-default pt-6">
            <div className="p-2.5 bg-accent-subtle border border-accent-subtle-border text-text-primary rounded-lg w-fit">
              <Search className="h-4.5 w-4.5" />
            </div>
            <h3 className="font-semibold text-base text-text-primary">Conceptual Retrieval</h3>
            <p className="text-xs text-text-muted leading-relaxed">
              Bridges keyword and dense vector-space algorithms. AKASHA queries semantic intention to locate exact fragments without requiring perfect string matches.
            </p>
          </div>
          {/* Card 2 */}
          <div className="space-y-4 border-t border-border-default pt-6">
            <div className="p-2.5 bg-accent-subtle border border-accent-subtle-border text-text-primary rounded-lg w-fit">
              <Sparkles className="h-4.5 w-4.5" />
            </div>
            <h3 className="font-semibold text-base text-text-primary">Grounded Synthesis</h3>
            <p className="text-xs text-text-muted leading-relaxed">
              Ask deep questions and get precise, synthesized answers backed by interactive citations directly tied to source file segments.
            </p>
          </div>
          {/* Card 3 */}
          <div className="space-y-4 border-t border-border-default pt-6">
            <div className="p-2.5 bg-accent-subtle border border-accent-subtle-border text-text-primary rounded-lg w-fit">
              <Cpu className="h-4.5 w-4.5" />
            </div>
            <h3 className="font-semibold text-base text-text-primary">OCR & Vision Extraction</h3>
            <p className="text-xs text-text-muted leading-relaxed">
              Text in screenshot logs, scanned receipts, and PDF charts is automatically digitized, parsed, and indexed using WebAssembly-powered OCR.
            </p>
          </div>
          {/* Card 4 */}
          <div className="space-y-4 border-t border-border-default pt-6">
            <div className="p-2.5 bg-accent-subtle border border-accent-subtle-border text-text-primary rounded-lg w-fit">
              <Laptop className="h-4.5 w-4.5" />
            </div>
            <h3 className="font-semibold text-base text-text-primary">Local-First Privacy</h3>
            <p className="text-xs text-text-muted leading-relaxed">
              True digital sovereignty. Your files, vector embeddings, and search logs never leave your device, running completely offline inside Docker.
            </p>
          </div>
        </div>
      </section>

      {/* High-Contrast CTA Banner */}
      <section className="border-t border-border-default bg-text-primary text-bg-base py-24 z-10 select-none text-center">
        <div className="max-w-4xl mx-auto px-6 space-y-6">
          <h2 className="font-serif text-5xl md:text-6xl font-normal tracking-tight">
            Reclaim your digital footprint.
          </h2>
          <p className="text-xs uppercase tracking-widest opacity-80 font-semibold font-mono">
            Local. Sovereign. Forever.
          </p>
          <div className="pt-4 flex justify-center">
            <Link
              to={accessToken ? "/dashboard" : "/register"}
              className="flex items-center gap-2 px-8 py-4 border border-bg-base bg-bg-base text-text-primary hover:bg-transparent hover:text-bg-base text-xs font-bold uppercase tracking-widest transition-all ease-motion cursor-pointer rounded-lg group"
            >
              {accessToken ? "Go to Dashboard" : "Get started"}
              <ArrowRight className="h-3.5 w-3.5 transition-transform group-hover:translate-x-1 shrink-0" />
            </Link>
          </div>
        </div>
      </section>

      {/* Footer */}
      <footer className="h-20 border-t border-border-default bg-bg-base flex items-center justify-between px-6 md:px-12 z-10 text-xs text-text-muted font-sans select-none">
        <div>
          © 2026 AKASHA · Ingest. Synthesize. Recall.
        </div>
      </footer>
    </div>
  );
}
