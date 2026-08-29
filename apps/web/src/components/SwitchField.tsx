import * as Switch from "@radix-ui/react-switch";
import { useId } from "react";

export interface SwitchFieldProps {
  label: string;
  description?: string;
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
  disabled?: boolean;
}

export function SwitchField({
  label,
  description,
  checked,
  onCheckedChange,
  disabled = false,
}: SwitchFieldProps) {
  const identifier = useId();
  const descriptionId = `${identifier}-description`;
  return (
    <div className="flex items-start justify-between gap-4 rounded-[var(--radius-medium)] border border-[var(--border-subtle)] bg-[var(--surface-input)] px-3 py-2.5">
      <div className="min-w-0">
        <label htmlFor={identifier} className="text-sm font-medium text-[var(--text-primary)]">
          {label}
        </label>
        {description !== undefined && (
          <p id={descriptionId} className="mt-0.5 text-xs text-[var(--text-muted)]">
            {description}
          </p>
        )}
      </div>
      <Switch.Root
        id={identifier}
        checked={checked}
        onCheckedChange={onCheckedChange}
        disabled={disabled}
        aria-describedby={description !== undefined ? descriptionId : undefined}
        className="relative h-5 w-9 shrink-0 rounded-full border border-[var(--border-strong)] bg-[var(--surface-sunken)] transition-colors data-[state=checked]:border-[var(--accent-primary)] data-[state=checked]:bg-[var(--accent-primary-strong)] disabled:opacity-50"
      >
        <Switch.Thumb className="block size-4 translate-x-0.5 rounded-full bg-[var(--text-primary)] transition-transform data-[state=checked]:translate-x-4 data-[state=checked]:bg-[var(--text-inverted)]" />
      </Switch.Root>
    </div>
  );
}
