import type { ReactNode } from "react";
import { AlertTriangle, Inbox, Loader2, ShieldAlert } from "lucide-react";

import { Button } from "./Button";
import { cx } from "./classNames";

export interface LoadingStateProps {
  label: string;
  className?: string;
}

export function LoadingState({ label, className }: LoadingStateProps) {
  return (
    <div
      role="status"
      aria-live="polite"
      className={cx(
        "flex flex-col items-center justify-center gap-3 px-6 py-12 text-center text-[var(--text-muted)]",
        className,
      )}
    >
      <Loader2 aria-hidden className="size-6 animate-spin text-[var(--accent-primary)]" />
      <p className="text-sm">{label}</p>
    </div>
  );
}

export interface EmptyStateProps {
  title: string;
  description: string;
  action?: ReactNode;
  icon?: ReactNode;
}

export function EmptyState({ title, description, action, icon }: EmptyStateProps) {
  return (
    <div className="flex flex-col items-center justify-center gap-3 px-6 py-14 text-center">
      <span className="rounded-full border border-[var(--border-subtle)] bg-[var(--surface-overlay)] p-3 text-[var(--text-muted)]">
        {icon ?? <Inbox aria-hidden className="size-5" />}
      </span>
      <h3 className="text-sm font-semibold text-[var(--text-primary)]">{title}</h3>
      <p className="max-w-md text-sm text-[var(--text-muted)]">{description}</p>
      {action}
    </div>
  );
}

export interface ErrorStateProps {
  title: string;
  message: string;
  remedy?: string | null;
  lastDurableEvent?: string | null;
  sourceChangesPossible?: boolean;
  evidenceHref?: string;
  onRetry?: () => void;
  diagnostic?: string;
}

export function ErrorState({
  title,
  message,
  remedy,
  lastDurableEvent,
  sourceChangesPossible,
  evidenceHref,
  onRetry,
  diagnostic,
}: ErrorStateProps) {
  const copyDiagnostic = () => {
    const payload = diagnostic ?? `${title}\n${message}\n${remedy ?? ""}`;
    void navigator.clipboard.writeText(payload);
  };
  return (
    <div
      role="alert"
      className="flex flex-col gap-3 rounded-[var(--radius-large)] border border-[color-mix(in_srgb,var(--state-failure)_45%,transparent)] bg-[var(--state-failure-surface)] p-4"
    >
      <div className="flex items-start gap-3">
        <AlertTriangle aria-hidden className="mt-0.5 size-5 shrink-0 text-[var(--state-failure)]" />
        <div className="min-w-0">
          <h3 className="text-sm font-semibold text-[var(--text-primary)]">{title}</h3>
          <p className="mt-1 text-sm text-[var(--text-secondary)]">{message}</p>
        </div>
      </div>
      <dl className="grid gap-1 text-xs text-[var(--text-secondary)]">
        {sourceChangesPossible !== undefined && (
          <div className="flex gap-2">
            <dt className="font-medium">Source changes</dt>
            <dd>
              {sourceChangesPossible
                ? "Candidate worktrees may contain changes. Your repository branch is untouched."
                : "No candidate source was changed."}
            </dd>
          </div>
        )}
        {lastDurableEvent !== undefined && lastDurableEvent !== null && (
          <div className="flex gap-2">
            <dt className="font-medium">Last durable event</dt>
            <dd className="truncate">{lastDurableEvent}</dd>
          </div>
        )}
        {remedy !== undefined && remedy !== null && (
          <div className="flex gap-2">
            <dt className="font-medium">Next action</dt>
            <dd>{remedy}</dd>
          </div>
        )}
      </dl>
      <div className="flex flex-wrap items-center gap-2">
        {onRetry !== undefined && (
          <Button tone="secondary" size="small" onClick={onRetry}>
            Try again
          </Button>
        )}
        {evidenceHref !== undefined && (
          <a
            href={evidenceHref}
            className="inline-flex h-8 items-center rounded-[var(--radius-medium)] border border-[var(--border-subtle)] px-3 text-[13px] text-[var(--text-secondary)] hover:text-[var(--text-primary)]"
          >
            Open evidence
          </a>
        )}
        <Button
          tone="ghost"
          size="small"
          onClick={copyDiagnostic}
          icon={<ShieldAlert aria-hidden className="size-3.5" />}
        >
          Copy redacted diagnostic
        </Button>
      </div>
    </div>
  );
}
