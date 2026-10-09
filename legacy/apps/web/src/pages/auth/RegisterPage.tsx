import { useEffect, useState } from "react";
import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { z } from "zod";
import { useMutation } from "@tanstack/react-query";
import { Link, useNavigate } from "react-router";
import { api, ApiError } from "../../lib/api";
import { useAuthStore } from "../../store/auth.store";
import { Loader2, AlertCircle, CheckCircle } from "lucide-react";
import { CanvasBackground } from "../../components/ui/CanvasBackground";

// Client-side schema validation using Zod
const registerSchema = z
  .object({
    email: z.string().email("Enter a valid email address"),
    password: z.string().min(8, "Password must be at least 8 characters long"),
    confirmPassword: z.string().min(8, "Confirm password must be at least 8 characters long"),
  })
  .refine((data) => data.password === data.confirmPassword, {
    message: "Passwords do not match",
    path: ["confirmPassword"],
  });

type RegisterFields = z.infer<typeof registerSchema>;

export function RegisterPage() {
  const { accessToken } = useAuthStore();
  const navigate = useNavigate();
  const [isSuccess, setIsSuccess] = useState(false);

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
  } = useForm<RegisterFields>({
    resolver: zodResolver(registerSchema),
  });

  const { mutate: signUp, isPending, error } = useMutation({
    mutationFn: async (data: Omit<RegisterFields, "confirmPassword">) => {
      const response = await api.post("/api/v1/auth/register", data);
      return response;
    },
    onSuccess: () => {
      setIsSuccess(true);
      setTimeout(() => {
        navigate("/login");
      }, 2500);
    },
  });

  const onSubmit = (data: RegisterFields) => {
    signUp({ email: data.email, password: data.password });
  };

  const getErrorMessage = (err: Error | null): string => {
    if (!err) return "";
    if (err instanceof ApiError) return err.message;
    return err.message || "Registration failed. Please try again.";
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

        {/* Register Panel */}
        <div className="bg-bg-surface border border-border-default rounded-xl p-8 shadow-xl">
          <h3 className="font-serif text-3xl text-text-primary tracking-tight mb-6 text-center">
            Create your account.
          </h3>

          {isSuccess ? (
            <div className="py-6 flex flex-col items-center justify-center text-center gap-4 animate-fade-in">
              <div className="h-12 w-12 rounded-full bg-state-success/10 border border-state-success/25 flex items-center justify-center animate-scale">
                <CheckCircle className="h-6 w-6 text-state-success" />
              </div>
              <div>
                <p className="text-sm font-semibold text-text-primary">Account Created Successfully</p>
                <p className="text-xs text-text-muted mt-1">Redirecting you to sign in...</p>
              </div>
            </div>
          ) : (
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
                  autoComplete="new-password"
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

              {/* Confirm Password Field */}
              <div className="space-y-1.5 text-left">
                <label htmlFor="confirmPassword" className="text-[10px] uppercase font-semibold tracking-wider text-text-muted">
                  Confirm Password
                </label>
                <input
                  id="confirmPassword"
                  type="password"
                  autoComplete="new-password"
                  className={`w-full bg-bg-base border rounded-lg px-3.5 py-2.5 text-xs text-text-primary placeholder:text-text-dim outline-none transition-all ${
                    errors.confirmPassword
                      ? "border-state-error focus:border-state-error focus:ring-1 focus:ring-state-error"
                      : "border-border-default focus:border-accent-primary focus:ring-1 focus:ring-accent-primary"
                  }`}
                  placeholder="••••••••••••"
                  {...register("confirmPassword")}
                />
                {errors.confirmPassword && (
                  <p className="text-[10px] text-state-error font-medium">{errors.confirmPassword.message}</p>
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
                    Registering...
                  </>
                ) : (
                  "Create account"
                )}
              </button>
            </form>
          )}
        </div>

        {/* Redirection Links */}
        <p className="text-center text-xs text-text-muted mt-6">
          Already have an account?{" "}
          <Link to="/login" className="text-accent-primary hover:text-accent-hover font-semibold transition-colors">
            Sign in
          </Link>
        </p>
      </div>
    </div>
  );
}
