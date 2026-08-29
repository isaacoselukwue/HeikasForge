import type { ReactNode } from "react";

import { cx } from "./classNames";

export interface PanelProps {
  title?: ReactNode;
  description?: ReactNode;
  actions?: ReactNode;
  children: ReactNode;
  className?: string;
  bodyClassName?: string;
  as?: "section" | "article" | "div";
  labelledBy?: string;
}

export function Panel({
  title,
  description,
  actions,
  children,
  className,
  bodyClassName,
  as = "section",
  labelledBy,
}: PanelProps) {
  const Element = as;
  return (
    <Element
      className={cx("surface-panel flex min-h-0 flex-col", className)}
      aria-labelledby={labelledBy}
    >
      {(title !== undefined || actions !== undefined) && (
        <header className="flex items-start justify-between gap-4 border-b border-[var(--border-subtle)] px-4 py-3">
          <div className="min-w-0">
            {title !== undefined && (
              <h2
                id={labelledBy}
                className="truncate text-sm font-semibold text-[var(--text-primary)]"
              >
                {title}
              </h2>
            )}
            {description !== undefined && (
              <p className="mt-0.5 text-xs text-[var(--text-muted)]">{description}</p>
            )}
          </div>
          {actions !== undefined && (
            <div className="flex shrink-0 items-center gap-2">{actions}</div>
          )}
        </header>
      )}
      <div className={cx("min-h-0 flex-1 p-4", bodyClassName)}>{children}</div>
    </Element>
  );
}
