import type { ReactElement, ReactNode } from "react";
import { render } from "@testing-library/react";
import type { RenderResult } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import axe from "axe-core";

import { ThemeProvider } from "@/app/theme";
import { TooltipProvider } from "@/components/Tooltip";

export function createTestQueryClient(): QueryClient {
  return new QueryClient({
    defaultOptions: {
      queries: { retry: false, gcTime: 0, staleTime: 0 },
      mutations: { retry: false },
    },
  });
}

export function Wrapper({ children }: { children: ReactNode }) {
  return (
    <ThemeProvider>
      <QueryClientProvider client={createTestQueryClient()}>
        <TooltipProvider>{children}</TooltipProvider>
      </QueryClientProvider>
    </ThemeProvider>
  );
}

export function renderComponent(element: ReactElement): RenderResult {
  return render(element, { wrapper: Wrapper });
}

export async function findAccessibilityViolations(container: HTMLElement): Promise<axe.Result[]> {
  const results = await axe.run(container, {
    rules: {
      "color-contrast": { enabled: false },
      region: { enabled: false },
    },
  });
  return results.violations;
}

export function describeViolations(violations: axe.Result[]): string {
  return violations
    .map(
      (violation) => `${violation.id}: ${violation.help} (${String(violation.nodes.length)} nodes)`,
    )
    .join("\n");
}
