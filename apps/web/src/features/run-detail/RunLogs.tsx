import { useMemo, useState } from "react";

import { useLogs } from "@/api/queries";
import { VirtualList } from "@/components/VirtualList";
import { EmptyState, ErrorState, LoadingState } from "@/components/StateViews";
import { Badge } from "@/components/Badge";
import { inputClasses } from "@/components/Field";
import { Tabs } from "@/components/Tabs";
import { formatClockTime } from "@/app/format";

const LEVEL_TONE: Record<string, "neutral" | "warning" | "failure" | "info"> = {
  TRACE: "neutral",
  DEBUG: "neutral",
  INFO: "info",
  WARN: "warning",
  ERROR: "failure",
};

export function RunLogs({ runId }: { runId: string }) {
  const logs = useLogs(runId);
  const [level, setLevel] = useState("all");
  const [view, setView] = useState("readable");

  const records = useMemo(
    () =>
      (logs.data?.records ?? []).filter(
        (record) => level === "all" || record.level.toUpperCase() === level,
      ),
    [logs.data, level],
  );

  if (logs.isPending) {
    return <LoadingState label="Loading structured logs" />;
  }
  if (logs.isError) {
    return (
      <ErrorState
        title="The logs could not be loaded"
        message={logs.error.message}
        onRetry={() => {
          void logs.refetch();
        }}
      />
    );
  }

  return (
    <div className="flex min-h-0 flex-col gap-3">
      <div className="flex flex-wrap items-center gap-3">
        <label className="flex items-center gap-2 text-xs text-[var(--text-muted)]">
          <span>Level</span>
          <select
            className={`${inputClasses} w-36`}
            value={level}
            onChange={(event) => {
              setLevel(event.target.value);
            }}
          >
            <option value="all">All levels</option>
            <option value="INFO">Information</option>
            <option value="WARN">Warning</option>
            <option value="ERROR">Error</option>
          </select>
        </label>
        <span className="text-xs text-[var(--text-muted)]">
          {records.length} of {logs.data.total} records. Secrets are redacted before the browser
          receives them.
        </span>
      </div>

      <Tabs
        label="Log presentation"
        value={view}
        onValueChange={setView}
        className="min-h-0 flex-1"
        tabs={[
          {
            value: "readable",
            label: "Readable",
            content: (
              <VirtualList
                items={records}
                estimateSize={54}
                label="Structured run log"
                className="h-[380px]"
                emptyState={
                  <EmptyState
                    title="No log records yet"
                    description="Structured records appear as the dispatcher executes nodes."
                  />
                }
                renderItem={(record) => (
                  <div className="flex items-start gap-3 border-b border-[var(--border-subtle)] px-3 py-2">
                    <span className="w-16 shrink-0 font-mono text-[11px] text-[var(--text-muted)]">
                      {formatClockTime(record.recorded_at)}
                    </span>
                    <Badge tone={LEVEL_TONE[record.level.toUpperCase()] ?? "neutral"}>
                      {record.level}
                    </Badge>
                    <span className="min-w-0 flex-1 text-[13px] text-[var(--text-secondary)]">
                      <span className="text-[var(--text-muted)]">{record.target}</span>{" "}
                      {record.message}
                    </span>
                  </div>
                )}
              />
            ),
          },
          {
            value: "raw",
            label: "Raw JSON",
            content: (
              <pre className="scrollbar-slim h-[380px] overflow-auto rounded-[var(--radius-medium)] bg-[var(--surface-sunken)] p-3 text-[12px] leading-relaxed">
                {records.map((record) => JSON.stringify(record)).join("\n")}
              </pre>
            ),
          },
        ]}
      />
    </div>
  );
}
