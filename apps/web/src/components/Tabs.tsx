import * as TabsPrimitive from "@radix-ui/react-tabs";
import type { ReactNode } from "react";

import { cx } from "./classNames";

export interface TabDefinition {
  value: string;
  label: string;
  badge?: ReactNode;
  content: ReactNode;
}

export interface TabsProps {
  tabs: TabDefinition[];
  value: string;
  onValueChange: (value: string) => void;
  label: string;
  className?: string;
}

export function Tabs({ tabs, value, onValueChange, label, className }: TabsProps) {
  return (
    <TabsPrimitive.Root
      value={value}
      onValueChange={onValueChange}
      className={cx("flex min-h-0 flex-col", className)}
    >
      <TabsPrimitive.List
        aria-label={label}
        className="flex shrink-0 items-center gap-1 border-b border-[var(--border-subtle)] px-1"
      >
        {tabs.map((tab) => (
          <TabsPrimitive.Trigger
            key={tab.value}
            value={tab.value}
            className="inline-flex items-center gap-2 border-b-2 border-transparent px-3 py-2 text-[13px] font-medium text-[var(--text-muted)] transition-colors hover:text-[var(--text-primary)] data-[state=active]:border-[var(--accent-primary)] data-[state=active]:text-[var(--text-primary)]"
          >
            {tab.label}
            {tab.badge}
          </TabsPrimitive.Trigger>
        ))}
      </TabsPrimitive.List>
      {tabs.map((tab) => (
        <TabsPrimitive.Content
          key={tab.value}
          value={tab.value}
          className="min-h-0 flex-1 overflow-auto focus-visible:outline-none"
        >
          {tab.content}
        </TabsPrimitive.Content>
      ))}
    </TabsPrimitive.Root>
  );
}
