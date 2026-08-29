import { forwardRef } from "react";
import type { ButtonHTMLAttributes, ReactNode } from "react";

import { cx } from "./classNames";

export type ButtonTone = "primary" | "secondary" | "ghost" | "danger";
export type ButtonSize = "small" | "medium";

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  tone?: ButtonTone;
  size?: ButtonSize;
  icon?: ReactNode;
  busy?: boolean;
}

const toneClasses: Record<ButtonTone, string> = {
  primary:
    "bg-[var(--accent-primary-strong)] text-[var(--text-inverted)] hover:bg-[var(--accent-primary)] border-transparent",
  secondary:
    "bg-[var(--surface-overlay)] text-[var(--text-primary)] hover:bg-[var(--surface-sunken)] border-[var(--border-subtle)]",
  ghost:
    "bg-transparent text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:bg-[var(--surface-overlay)] border-transparent",
  danger:
    "bg-[var(--state-failure-surface)] text-[var(--state-failure)] hover:bg-[var(--state-failure)] hover:text-[var(--text-inverted)] border-[var(--state-failure)]",
};

const sizeClasses: Record<ButtonSize, string> = {
  small: "h-8 px-3 text-[13px] gap-1.5",
  medium: "h-9 px-4 text-sm gap-2",
};

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(function Button(
  {
    tone = "secondary",
    size = "medium",
    icon,
    busy = false,
    className,
    children,
    disabled,
    ...rest
  },
  ref,
) {
  return (
    <button
      ref={ref}
      type="button"
      disabled={disabled === true || busy}
      aria-busy={busy}
      className={cx(
        "inline-flex items-center justify-center rounded-[var(--radius-medium)] border font-medium transition-colors duration-[var(--duration-fast)] disabled:cursor-not-allowed disabled:opacity-55",
        toneClasses[tone],
        sizeClasses[size],
        className,
      )}
      {...rest}
    >
      {icon}
      {children}
    </button>
  );
});
