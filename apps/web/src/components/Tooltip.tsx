import * as TooltipPrimitive from "@radix-ui/react-tooltip";
import type { ReactNode } from "react";

export interface TooltipProps {
  content: string;
  children: ReactNode;
}

export function TooltipProvider({ children }: { children: ReactNode }) {
  return <TooltipPrimitive.Provider delayDuration={250}>{children}</TooltipPrimitive.Provider>;
}

export function Tooltip({ content, children }: TooltipProps) {
  return (
    <TooltipPrimitive.Root>
      <TooltipPrimitive.Trigger asChild>{children}</TooltipPrimitive.Trigger>
      <TooltipPrimitive.Portal>
        <TooltipPrimitive.Content
          sideOffset={6}
          className="z-50 max-w-72 rounded-[var(--radius-medium)] border border-[var(--border-subtle)] bg-[var(--surface-overlay)] px-2.5 py-1.5 text-xs text-[var(--text-secondary)] shadow-[var(--shadow-raised)]"
        >
          {content}
          <TooltipPrimitive.Arrow className="fill-[var(--surface-overlay)]" />
        </TooltipPrimitive.Content>
      </TooltipPrimitive.Portal>
    </TooltipPrimitive.Root>
  );
}
