import { useEffect } from "react";
import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { z } from "zod";
import { useMutation } from "@tanstack/react-query";
import { Link, useNavigate } from "react-router";
import { api, ApiError } from "../../lib/api";
import { useAuthStore } from "../../store/auth.store";
import { Loader2, AlertCircle } from "lucide-react";
import { CanvasBackground } from "../../components/ui/CanvasBackground";

// Client-side schema validation using Zod
const loginSchema = z.object({
  email: z.string().email("Enter a valid email address"),
  password: z.string().min(8, "Password must be at least 8 characters long"),
});

type LoginFields = z.infer<typeof loginSchema>;

export function LoginPage() {
  const { accessToken, setAuth } = useAuthStore();
  const navigate = useNavigate();

  // Redirect to dashboard immediately if already logged in
  useEffect(() => {
    if (accessToken) {
      navigate("/dashboard", { replace: true });
    }
  }, [accessToken, navigate]);

  const {
    register,
    handleSubmit,
    formState: { errors },
  } = useForm<LoginFields>({
    resolver: zodResolver(loginSchema),
  });

  const { mutate: login, isPending, error } = useMutation({
    mutationFn: async (data: LoginFields) => {
      const response = await api.post("/api/v1/auth/login", data);
      return response as { accessToken: string; user: { id: string; email: string } };
    },
    onSuccess: (data) => {
      setAuth(data.accessToken);
      navigate("/dashboard", { replace: true });
    },
  });

  const onSubmit = (data: LoginFields) => {
    login(data);
  };

  const getErrorMessage = (err: Error | null): string => {
    if (!err) return "";
    if (err instanceof ApiError) return err.message;
    return err.message || "Invalid credentials. Please try again.";
  };

  return (
    <div className="relative min-h-screen bg-bg-base flex flex-col items-center justify-center p-6 select-none overflow-hidden font-sans">
      {/* Immersive moderate intensity stars background layer */}
      <CanvasBackground intensity="moderate" />

      <div className="relative z-10 w-full max-w-[400px]">
        {/* Brand Block */}
        <div className="text-center mb-8">
          <Link to="/" className="inline-flex flex-col items-start gap-1 mb-6 hover:opacity-80 transition-opacity cursor-pointer" aria-label="AKASHA Logo">
            <div className="h-[2px] w-[24px] bg-accent-primary rounded-full"></div>
            <div className="h-[2px] w-[18px] bg-accent-primary rounded-full"></div>
            <div className="h-[2px] w-[12px] bg-accent-primary rounded-full"></div>
          </Link>
          <h2 className="text-sm font-semibold tracking-[0.15em] uppercase text-text-primary">
            AKASHA
          </h2>
          <p className="text-[10px] font-semibold tracking-[0.12em] uppercase text-accent-primary mt-1">
            Index. Retrieve. Remember.
          </p>
        </div>

        {/* Login Panel */}
        <div className="bg-bg-surface border border-border-default rounded-xl p-8 shadow-xl">
          <h3 className="font-serif text-3xl text-text-primary tracking-tight mb-6 text-center">
            Welcome back.
          </h3>

          <form onSubmit={handleSubmit(onSubmit)} className="space-y-5">
            {/* Form Error Callout */}
            {error && (
              <div className="bg-state-error/10 border border-state-error/25 text-state-error text-xs rounded-lg p-3.5 flex items-start gap-2.5">
                <AlertCircle className="h-4.5 w-4.5 mt-0.5 shrink-0" />
                <p className="leading-normal select-text selection:bg-accent-primary/20">{getErrorMessage(error)}</p>
              </div>
            )}

            {/* Email Field */}
            <div className="space-y-1.5 text-left">
              <label htmlFor="email" className="text-[10px] uppercase font-semibold tracking-wider text-text-muted">
                Email
              </label>
              <input
                id="email"
                type="email"
                autoComplete="email"
                className={`w-full bg-bg-base border rounded-lg px-3.5 py-2.5 text-xs text-text-primary placeholder:text-text-dim outline-none transition-all ${
                  errors.email
                    ? "border-state-error focus:border-state-error focus:ring-1 focus:ring-state-error"
                    : "border-border-default focus:border-accent-primary focus:ring-1 focus:ring-accent-primary"
                }`}
                placeholder="you@domain.com"
                {...register("email")}
              />
              {errors.email && (
                <p className="text-[10px] text-state-error font-medium">{errors.email.message}</p>
              )}
            </div>

            {/* Password Field */}
            <div className="space-y-1.5 text-left">
              <label htmlFor="password" className="text-[10px] uppercase font-semibold tracking-wider text-text-muted">
                Password
              </label>
              <input
                id="password"
                type="password"
                autoComplete="current-password"
                className={`w-full bg-bg-base border rounded-lg px-3.5 py-2.5 text-xs text-text-primary placeholder:text-text-dim outline-none transition-all ${
                  errors.password
                    ? "border-state-error focus:border-state-error focus:ring-1 focus:ring-state-error"
                    : "border-border-default focus:border-accent-primary focus:ring-1 focus:ring-accent-primary"
                }`}
                placeholder="••••••••••••"
                {...register("password")}
              />
              {errors.password && (
                <p className="text-[10px] text-state-error font-medium">{errors.password.message}</p>
              )}
            </div>

            {/* Submit Control */}
            <button
              type="submit"
              disabled={isPending}
              className="w-full bg-accent-primary hover:bg-accent-hover text-text-primary border border-transparent rounded-lg py-2.5 text-xs font-semibold uppercase tracking-wider transition-all disabled:opacity-50 flex items-center justify-center gap-2 cursor-pointer mt-2"
            >
              {isPending ? (
                <>
                  <Loader2 className="h-4.5 w-4.5 animate-spin" />
                  Signing in...
                </>
              ) : (
                "Sign in"
              )}
            </button>
          </form>
        </div>

        {/* Redirection Links */}
        <p className="text-center text-xs text-text-muted mt-6">
          No account?{" "}
          <Link to="/register" className="text-accent-primary hover:text-accent-hover font-semibold transition-colors">
            Create one
          </Link>
        </p>
      </div>
    </div>
  );
}
