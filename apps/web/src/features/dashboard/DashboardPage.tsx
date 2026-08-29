import { useMemo, useState } from "react";
import { Link } from "@tanstack/react-router";
import { CircleDot, GitBranch, PlusCircle, Search, Timer } from "lucide-react";

import { useRuns } from "@/api/queries";
import { Badge } from "@/components/Badge";
import { runStatusTone } from "@/components/tone";
import { Button } from "@/components/Button";
import { EmptyState, ErrorState, LoadingState } from "@/components/StateViews";
import { Panel } from "@/components/Panel";
import { inputClasses } from "@/components/Field";
import { formatDuration, formatRelative, shortRunId } from "@/app/format";
import type { RunSummary } from "@/generated/api-types";

const STATUS_FILTERS = [
  { value: "all", label: "All statuses" },
  { value: "active", label: "Active" },
  { value: "awaiting_plan_approval", label: "Awaiting plan approval" },
  { value: "awaiting_commit_approval", label: "Awaiting commit approval" },
  { value: "succeeded", label: "Succeeded" },
  { value: "failed", label: "Failed" },
  { value: "exhausted", label: "Exhausted" },
  { value: "cancelled", label: "Cancelled" },
];

const TERMINAL = ["succeeded", "failed", "cancelled", "exhausted"];

export function DashboardPage() {
  const runs = useRuns();
  const [search, setSearch] = useState("");
  const [status, setStatus] = useState("all");
  const [repository, setRepository] = useState("all");

  const repositories = useMemo(() => {
    const unique = new Set((runs.data ?? []).map((run) => run.repository_path));
    return ["all", ...Array.from(unique).sort()];
  }, [runs.data]);

  const filtered = useMemo(() => {
    const needle = search.trim().toLowerCase();
    return (runs.data ?? []).filter((run) => {
      if (status === "active" && TERMINAL.includes(run.status)) {
        return false;
      }
      if (status !== "all" && status !== "active" && run.status !== status) {
        return false;
      }
      if (repository !== "all" && run.repository_path !== repository) {
        return false;
      }
      if (needle.length === 0) {
        return true;
      }
      return (
        run.run_id.toLowerCase().includes(needle) ||
        run.task_title.toLowerCase().includes(needle) ||
        run.repository_path.toLowerCase().includes(needle)
      );
    });
  }, [runs.data, search, status, repository]);

  const active = filtered.filter((run) => !TERMINAL.includes(run.status));
  const history = filtered.filter((run) => TERMINAL.includes(run.status));

  if (runs.isPending) {
    return <LoadingState label="Loading runs" />;
  }

  if (runs.isError) {
    return (
      <div className="p-6">
        <ErrorState
          title="The run list could not be loaded"
          message={runs.error.message}
          remedy="Confirm that the local orchestrator is still running."
          onRetry={() => {
            void runs.refetch();
          }}
        />
      </div>
    );
  }

  return (
    <div className="mx-auto flex max-w-[1600px] flex-col gap-4 p-6">
      <div className="flex flex-wrap items-end justify-between gap-3">
        <div>
          <h1 className="text-xl font-semibold text-[var(--text-primary)]">Runs</h1>
          <p className="mt-1 text-sm text-[var(--text-muted)]">
            Every orchestration run on this machine, newest first.
          </p>
        </div>
        <Button
          tone="primary"
          icon={<PlusCircle aria-hidden className="size-4" />}
          onClick={() => {
            window.location.assign("/new");
          }}
        >
          New run
        </Button>
      </div>

      <div className="flex flex-wrap items-center gap-3">
        <div className="relative min-w-64 flex-1">
          <Search
            aria-hidden
            className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-[var(--text-muted)]"
          />
          <input
            className={`${inputClasses} pl-9`}
            placeholder="Search by run identifier, task or repository"
            aria-label="Search runs"
            value={search}
            onChange={(event) => {
              setSearch(event.target.value);
            }}
          />
        </div>
        <label className="flex items-center gap-2 text-xs text-[var(--text-muted)]">
          <span>Status</span>
          <select
            className={`${inputClasses} w-56`}
            value={status}
            onChange={(event) => {
              setStatus(event.target.value);
            }}
          >
            {STATUS_FILTERS.map((option) => (
              <option key={option.value} value={option.value}>
                {option.label}
              </option>
            ))}
          </select>
        </label>
        <label className="flex items-center gap-2 text-xs text-[var(--text-muted)]">
          <span>Repository</span>
          <select
            className={`${inputClasses} w-72`}
            value={repository}
            onChange={(event) => {
              setRepository(event.target.value);
            }}
          >
            {repositories.map((option) => (
              <option key={option} value={option}>
                {option === "all" ? "All repositories" : option}
              </option>
            ))}
          </select>
        </label>
      </div>

      {filtered.length === 0 ? (
        <Panel title="No runs match the current filters">
          <EmptyState
            title="Nothing to show yet"
            description="Create a run to plan a change, review it and let the orchestrator apply the deterministic gates."
            action={
              <Link
                to="/new"
                className="inline-flex h-9 items-center rounded-[var(--radius-medium)] bg-[var(--accent-primary-strong)] px-4 text-sm font-medium text-[var(--text-inverted)]"
              >
                Start a run
              </Link>
            }
          />
        </Panel>
      ) : (
        <div className="flex flex-col gap-6">
          {active.length > 0 && (
            <section aria-labelledby="active-runs-heading" className="flex flex-col gap-3">
              <h2
                id="active-runs-heading"
                className="text-sm font-semibold text-[var(--text-secondary)]"
              >
                Active
              </h2>
              <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-3">
                {active.map((run) => (
                  <RunCard key={run.run_id} run={run} />
                ))}
              </div>
            </section>
          )}
          {history.length > 0 && (
            <section aria-labelledby="history-runs-heading" className="flex flex-col gap-3">
              <h2
                id="history-runs-heading"
                className="text-sm font-semibold text-[var(--text-secondary)]"
              >
                History
              </h2>
              <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-3">
                {history.map((run) => (
                  <RunCard key={run.run_id} run={run} />
                ))}
              </div>
            </section>
          )}
        </div>
      )}
    </div>
  );
}

function RunCard({ run }: { run: RunSummary }) {
  const progress = run.candidate_progress;
  const completed = progress.eligible + progress.ineligible;
  const percentage = progress.total === 0 ? 0 : Math.round((completed / progress.total) * 100);
  return (
    <Link
      to="/runs/$runId"
      params={{ runId: run.run_id }}
      className="surface-panel flex flex-col gap-3 p-4 transition-colors hover:border-[var(--border-strong)]"
    >
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <h3 className="truncate text-sm font-semibold text-[var(--text-primary)]">
            {run.task_title}
          </h3>
          <p className="mt-0.5 truncate text-xs text-[var(--text-muted)]">{run.repository_path}</p>
        </div>
        <Badge tone={runStatusTone(run.status)}>{run.status_label}</Badge>
      </div>

      <dl className="grid grid-cols-2 gap-2 text-xs text-[var(--text-muted)]">
        <div className="flex items-center gap-1.5">
          <CircleDot aria-hidden className="size-3.5" />
          <dt className="sr-only">Current node</dt>
          <dd>
            {run.current_nodes.length > 0
              ? run.current_nodes.map((node) => node.replace(/_/g, " ")).join(", ")
              : "no node in flight"}
          </dd>
        </div>
        <div className="flex items-center gap-1.5">
          <Timer aria-hidden className="size-3.5" />
          <dt className="sr-only">Elapsed</dt>
          <dd>{formatDuration(run.elapsed)}</dd>
        </div>
        <div className="flex items-center gap-1.5">
          <GitBranch aria-hidden className="size-3.5" />
          <dt className="sr-only">Run identifier</dt>
          <dd className="font-mono">{shortRunId(run.run_id)}</dd>
        </div>
        <div>
          <dt className="sr-only">Last update</dt>
          <dd>{formatRelative(run.updated_at)}</dd>
        </div>
      </dl>

      <div>
        <div className="flex items-center justify-between text-xs text-[var(--text-muted)]">
          <span>
            Candidates {completed} of {progress.total}
          </span>
          <span>{progress.eligible} eligible</span>
        </div>
        <div
          role="progressbar"
          aria-valuenow={percentage}
          aria-valuemin={0}
          aria-valuemax={100}
          aria-label="Candidate completion"
          className="mt-1.5 h-1.5 overflow-hidden rounded-full bg-[var(--surface-sunken)]"
        >
          <div
            className="h-full rounded-full bg-[var(--accent-primary)] transition-[width] duration-[var(--duration-medium)]"
            style={{ width: `${String(percentage)}%` }}
          />
        </div>
      </div>

      {run.last_event_summary !== null && (
        <p className="truncate text-xs text-[var(--text-secondary)]">{run.last_event_summary}</p>
      )}
    </Link>
  );
}
