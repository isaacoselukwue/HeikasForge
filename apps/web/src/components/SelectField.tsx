import * as Select from "@radix-ui/react-select";
import { Check, ChevronDown } from "lucide-react";

import { Field } from "./Field";

export interface SelectOption {
  value: string;
  label: string;
  description?: string;
  disabled?: boolean;
}

export interface SelectFieldProps {
  label: string;
  hint?: string;
  error?: string | null;
  value: string;
  options: SelectOption[];
  onValueChange: (value: string) => void;
}

export function SelectField({
  label,
  hint,
  error,
  value,
  options,
  onValueChange,
}: SelectFieldProps) {
  return (
    <Field label={label} hint={hint} error={error}>
      {(identifier, describedBy) => (
        <Select.Root value={value} onValueChange={onValueChange}>
          <Select.Trigger
            id={identifier}
            aria-describedby={describedBy}
            className="flex h-9 w-full items-center justify-between gap-2 rounded-[var(--radius-medium)] border border-[var(--border-subtle)] bg-[var(--surface-input)] px-3 text-sm text-[var(--text-primary)] data-[placeholder]:text-[var(--text-muted)]"
          >
            <Select.Value />
            <Select.Icon>
              <ChevronDown aria-hidden className="size-4 text-[var(--text-muted)]" />
            </Select.Icon>
          </Select.Trigger>
          <Select.Portal>
            <Select.Content
              position="popper"
              sideOffset={6}
              className="z-50 max-h-72 min-w-[var(--radix-select-trigger-width)] overflow-hidden rounded-[var(--radius-medium)] border border-[var(--border-subtle)] bg-[var(--surface-overlay)] shadow-[var(--shadow-raised)]"
            >
              <Select.Viewport className="p-1">
                {options.map((option) => (
                  <Select.Item
                    key={option.value}
                    value={option.value}
                    disabled={option.disabled}
                    className="flex cursor-default select-none items-start gap-2 rounded-[var(--radius-small)] px-2 py-1.5 text-sm text-[var(--text-primary)] data-[disabled]:opacity-50 data-[highlighted]:bg-[var(--surface-sunken)] data-[highlighted]:outline-none"
                  >
                    <Select.ItemIndicator className="mt-0.5">
                      <Check aria-hidden className="size-3.5 text-[var(--accent-primary)]" />
                    </Select.ItemIndicator>
                    <span className="flex flex-col">
                      <Select.ItemText>{option.label}</Select.ItemText>
                      {option.description !== undefined && (
                        <span className="text-xs text-[var(--text-muted)]">
                          {option.description}
                        </span>
                      )}
                    </span>
                  </Select.Item>
                ))}
              </Select.Viewport>
            </Select.Content>
          </Select.Portal>
        </Select.Root>
      )}
    </Field>
  );
}
