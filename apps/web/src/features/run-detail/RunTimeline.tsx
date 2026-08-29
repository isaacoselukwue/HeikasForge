import { useMemo, useState } from "react";

import { VirtualList } from "@/components/VirtualList";
import { EmptyState } from "@/components/StateViews";
import { Badge } from "@/components/Badge";
import { inputClasses } from "@/components/Field";
import { formatClockTime, formatDuration } from "@/app/format";
import type { TimelineEntry } from "@/generated/api-types";

const LEVEL_TONE = {
  information: "neutral",
  success: "success",
  warning: "warning",
  failure: "failure",
} as const;

export interface RunTimelineProps {
  entries: TimelineEntry[];
  candidateFilter: string;
  onCandidateFilterChange: (value: string) => void;
  candidates: string[];
}

export function RunTimeline({
  entries,
  candidateFilter,
  onCandidateFilterChange,
  candidates,
}: RunTimelineProps) {
  const [levelFilter, setLevelFilter] = useState("all");

  const filtered = useMemo(
    () =>
      entries.filter((entry) => {
        if (levelFilter !== "all" && entry.level !== levelFilter) {
          return false;
        }
        if (candidateFilter !== "all" && entry.candidate_id !== candidateFilter) {
          return false;
        }
        return true;
      }),
    [entries, levelFilter, candidateFilter],
  );

  return (
    <div className="flex min-h-0 flex-col gap-3">
      <div className="flex flex-wrap items-center gap-3">
        <label className="flex items-center gap-2 text-xs text-[var(--text-muted)]">
          <span>Level</span>
          <select
            className={`${inputClasses} w-40`}
            value={levelFilter}
            onChange={(event) => {
              setLevelFilter(event.target.value);
            }}
          >
            <option value="all">All levels</option>
            <option value="information">Information</option>
            <option value="success">Success</option>
            <option value="warning">Warning</option>
            <option value="failure">Failure</option>
          </select>
        </label>
        <label className="flex items-center gap-2 text-xs text-[var(--text-muted)]">
          <span>Candidate</span>
          <select
            className={`${inputClasses} w-52`}
            value={candidateFilter}
            onChange={(event) => {
              onCandidateFilterChange(event.target.value);
            }}
          >
            <option value="all">All candidates</option>
            {candidates.map((candidate) => (
              <option key={candidate} value={candidate}>
                {candidate}
              </option>
            ))}
          </select>
        </label>
        <span className="text-xs text-[var(--text-muted)]">
          {filtered.length} of {entries.length} events
        </span>
      </div>

      <VirtualList
        items={filtered}
        estimateSize={64}
        label="Run timeline"
        className="h-[420px] rounded-[var(--radius-medium)] border border-[var(--border-subtle)]"
        emptyState={
          <EmptyState
            title="No events match the filters"
            description="Relax the level or candidate filter to see the recorded transitions."
          />
        }
        renderItem={(entry) => (
          <div className="flex items-start gap-3 border-b border-[var(--border-subtle)] px-3 py-2.5">
            <span className="w-16 shrink-0 font-mono text-[11px] text-[var(--text-muted)]">
              {formatClockTime(entry.recorded_at)}
            </span>
            <Badge tone={LEVEL_TONE[entry.level]}>{entry.level}</Badge>
            <div className="min-w-0 flex-1">
              <p className="text-[13px] text-[var(--text-primary)]">{entry.summary}</p>
              <p className="mt-0.5 text-[11px] text-[var(--text-muted)]">
                sequence {entry.sequence}
                {entry.node_label != null ? ` · ${entry.node_label}` : ""}
                {entry.candidate_id != null ? ` · ${entry.candidate_id}` : ""}
                {entry.attempt != null ? ` · attempt ${String(entry.attempt)}` : ""}
                {entry.duration != null ? ` · ${formatDuration(entry.duration)}` : ""}
              </p>
            </div>
          </div>
        )}
      />
    </div>
  );
}
