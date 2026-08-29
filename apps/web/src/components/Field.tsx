import type { ReactNode } from "react";
import { useId } from "react";
import * as Label from "@radix-ui/react-label";

import { cx } from "./classNames";

export interface FieldProps {
  label: string;
  hint?: string;
  error?: string | null;
  children: (identifier: string, describedBy: string | undefined) => ReactNode;
  className?: string;
}

export function Field({ label, hint, error, children, className }: FieldProps) {
  const identifier = useId();
  const hintId = `${identifier}-hint`;
  const errorId = `${identifier}-error`;
  const describedBy = [hint !== undefined ? hintId : null, error != null ? errorId : null]
    .filter((value): value is string => value !== null)
    .join(" ");
  return (
    <div className={cx("flex flex-col gap-1.5", className)}>
      <Label.Root htmlFor={identifier} className="text-xs font-medium text-[var(--text-secondary)]">
        {label}
      </Label.Root>
      {children(identifier, describedBy.length > 0 ? describedBy : undefined)}
      {hint !== undefined && (
        <p id={hintId} className="text-xs text-[var(--text-muted)]">
          {hint}
        </p>
      )}
      {error != null && (
        <p id={errorId} role="alert" className="text-xs font-medium text-[var(--state-failure)]">
          {error}
        </p>
      )}
    </div>
  );
}

export const inputClasses =
  "h-9 w-full rounded-[var(--radius-medium)] border border-[var(--border-subtle)] bg-[var(--surface-input)] px-3 text-sm text-[var(--text-primary)] placeholder:text-[var(--text-muted)] focus-visible:border-[var(--border-focus)]";

export const textAreaClasses =
  "min-h-32 w-full resize-y rounded-[var(--radius-medium)] border border-[var(--border-subtle)] bg-[var(--surface-input)] px-3 py-2 font-mono text-[13px] leading-relaxed text-[var(--text-primary)] placeholder:text-[var(--text-muted)] focus-visible:border-[var(--border-focus)]";
