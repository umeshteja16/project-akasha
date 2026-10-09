import { MoonIcon, SunIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Tooltip } from "@/components/ui/tooltip";
import { useTheme } from "@/lib/theme";
import { cn } from "@/lib/utils";

export function ThemeToggle({ className }: { className?: string }) {
  const { resolved, toggle } = useTheme();
  const next = resolved === "dark" ? "light" : "dark";
  return (
    <Tooltip content={`Switch to ${next} theme`}>
      <Button
        variant="ghost"
        size="icon-sm"
        onClick={toggle}
        aria-label={`Switch to ${next} theme`}
        className={cn(className)}
      >
        {resolved === "dark" ? <SunIcon /> : <MoonIcon />}
      </Button>
    </Tooltip>
  );
}
