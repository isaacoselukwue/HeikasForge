import { useMemo } from "react";

import { cx } from "./classNames";
import { EmptyState } from "./StateViews";

export interface DiffViewerProps {
  patch: string;
  label: string;
  maximumLines?: number;
}

type LineKind = "meta" | "hunk" | "addition" | "removal" | "context";

function classify(line: string): LineKind {
  if (line.startsWith("@@")) {
    return "hunk";
  }
  if (
    line.startsWith("diff ") ||
    line.startsWith("index ") ||
    line.startsWith("--- ") ||
    line.startsWith("+++ ") ||
    line.startsWith("new file") ||
    line.startsWith("deleted file") ||
    line.startsWith("similarity ") ||
    line.startsWith("rename ") ||
    line.startsWith("GIT binary patch")
  ) {
    return "meta";
  }
  if (line.startsWith("+")) {
    return "addition";
  }
  if (line.startsWith("-")) {
    return "removal";
  }
  return "context";
}

const kindClasses: Record<LineKind, string> = {
  meta: "text-[var(--text-muted)]",
  hunk: "bg-[var(--state-info-surface)] text-[var(--state-info)]",
  addition:
    "bg-[color-mix(in_srgb,var(--state-success)_14%,transparent)] text-[var(--state-success)]",
  removal:
    "bg-[color-mix(in_srgb,var(--state-failure)_14%,transparent)] text-[var(--state-failure)]",
  context: "text-[var(--text-secondary)]",
};

export function DiffViewer({ patch, label, maximumLines = 4000 }: DiffViewerProps) {
  const lines = useMemo(() => {
    const all = patch.split(/\r?\n/);
    return all.slice(0, maximumLines);
  }, [patch, maximumLines]);

  if (patch.trim().length === 0) {
    return (
      <EmptyState
        title="No changes recorded"
        description="This candidate produced no difference against the baseline commit."
      />
    );
  }

  const truncated = patch.split(/\r?\n/).length > maximumLines;

  return (
    <div className="flex min-h-0 flex-col">
      <div
        role="group"
        aria-label={label}
        className="scrollbar-slim min-h-0 flex-1 overflow-auto rounded-[var(--radius-medium)] border border-[var(--border-subtle)] bg-[var(--surface-sunken)] font-mono text-[12px] leading-[1.55]"
      >
        <table className="w-full border-collapse">
          <caption className="sr-only">{label}</caption>
          <tbody>
            {lines.map((line, index) => {
              const kind = classify(line);
              return (
                <tr key={`${String(index)}-${line.slice(0, 12)}`} className={cx(kindClasses[kind])}>
                  <td className="w-12 select-none border-r border-[var(--border-subtle)] px-2 text-right text-[var(--text-muted)]">
                    {index + 1}
                  </td>
                  <td className="whitespace-pre-wrap break-all px-3">
                    {line.length > 0 ? line : " "}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
      {truncated && (
        <p className="mt-2 text-xs text-[var(--text-muted)]">
          The first {maximumLines} lines are shown. Export the run to inspect the complete patch.
        </p>
      )}
    </div>
  );
}
