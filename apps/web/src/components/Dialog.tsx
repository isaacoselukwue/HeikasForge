import * as DialogPrimitive from "@radix-ui/react-dialog";
import { X } from "lucide-react";
import type { ReactNode } from "react";

export interface DialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  description?: string;
  children: ReactNode;
  footer?: ReactNode;
}

export function Dialog({ open, onOpenChange, title, description, children, footer }: DialogProps) {
  return (
    <DialogPrimitive.Root open={open} onOpenChange={onOpenChange}>
      <DialogPrimitive.Portal>
        <DialogPrimitive.Overlay className="fixed inset-0 z-40 bg-black/60" />
        <DialogPrimitive.Content className="fixed left-1/2 top-1/2 z-50 max-h-[85vh] w-[min(680px,92vw)] -translate-x-1/2 -translate-y-1/2 overflow-auto rounded-[var(--radius-large)] border border-[var(--border-subtle)] bg-[var(--surface-overlay)] p-5 shadow-[var(--shadow-raised)]">
          <div className="mb-3 flex items-start justify-between gap-4">
            <div>
              <DialogPrimitive.Title className="text-base font-semibold text-[var(--text-primary)]">
                {title}
              </DialogPrimitive.Title>
              {description !== undefined && (
                <DialogPrimitive.Description className="mt-1 text-sm text-[var(--text-muted)]">
                  {description}
                </DialogPrimitive.Description>
              )}
            </div>
            <DialogPrimitive.Close
              aria-label="Close dialogue"
              className="rounded-[var(--radius-small)] p-1 text-[var(--text-muted)] hover:bg-[var(--surface-sunken)] hover:text-[var(--text-primary)]"
            >
              <X aria-hidden className="size-4" />
            </DialogPrimitive.Close>
          </div>
          {children}
          {footer !== undefined && <div className="mt-4 flex justify-end gap-2">{footer}</div>}
        </DialogPrimitive.Content>
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  );
}
