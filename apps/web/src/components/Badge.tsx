import type { ReactNode } from "react";

import { cx } from "./classNames";
import type { BadgeTone } from "./tone";

const toneClasses: Record<BadgeTone, string> = {
  neutral:
    "bg-[var(--state-neutral-surface)] text-[var(--state-neutral)] border-[var(--border-subtle)]",
  success:
    "bg-[var(--state-success-surface)] text-[var(--state-success)] border-[color-mix(in_srgb,var(--state-success)_40%,transparent)]",
  warning:
    "bg-[var(--state-warning-surface)] text-[var(--state-warning)] border-[color-mix(in_srgb,var(--state-warning)_40%,transparent)]",
  failure:
    "bg-[var(--state-failure-surface)] text-[var(--state-failure)] border-[color-mix(in_srgb,var(--state-failure)_40%,transparent)]",
  info: "bg-[var(--state-info-surface)] text-[var(--state-info)] border-[color-mix(in_srgb,var(--state-info)_40%,transparent)]",
  accent:
    "bg-[color-mix(in_srgb,var(--accent-primary)_18%,transparent)] text-[var(--accent-primary)] border-[color-mix(in_srgb,var(--accent-primary)_45%,transparent)]",
};

export interface BadgeProps {
  tone?: BadgeTone;
  children: ReactNode;
  icon?: ReactNode;
  className?: string;
  title?: string;
}

export function Badge({ tone = "neutral", children, icon, className, title }: BadgeProps) {
  return (
    <span
      title={title}
      className={cx(
        "inline-flex items-center gap-1.5 rounded-full border px-2.5 py-0.5 text-xs font-medium whitespace-nowrap",
        toneClasses[tone],
        className,
      )}
    >
      {icon}
      {children}
    </span>
  );
}
