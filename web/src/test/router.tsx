// Render a component inside a minimal router (for components with links).

import {
  createMemoryHistory,
  createRootRoute,
  createRouter,
  RouterProvider,
} from "@tanstack/react-router";
import { render } from "@testing-library/react";
import type { ReactNode } from "react";
import { TooltipProvider } from "@/components/ui/tooltip";

export async function renderWithRouter(ui: ReactNode) {
  const rootRoute = createRootRoute({ component: () => <TooltipProvider>{ui}</TooltipProvider> });
  const router = createRouter({
    routeTree: rootRoute,
    history: createMemoryHistory({ initialEntries: ["/"] }),
  });
  await router.load();
  const result = render(<RouterProvider router={router} />);
  return { ...result, router };
}
