import { useMemo, useState } from "react";
import { Link } from "@tanstack/react-router";
import {
  flexRender,
  getCoreRowModel,
  getSortedRowModel,
  useReactTable,
} from "@tanstack/react-table";
import type { ColumnDef, SortingState } from "@tanstack/react-table";
import { ArrowDown, ArrowUp, ArrowUpDown, GitCommitHorizontal, Trophy } from "lucide-react";

import { useApproveCommit, useCandidateDiff, useRunDetail } from "@/api/queries";
import { Badge } from "@/components/Badge";
import { candidateStatusTone } from "@/components/tone";
import { Button } from "@/components/Button";
import { DiffViewer } from "@/components/DiffViewer";
import { Panel } from "@/components/Panel";
import { EmptyState, ErrorState, LoadingState } from "@/components/StateViews";
import { inputClasses } from "@/components/Field";
import { formatDuration } from "@/app/format";
import type { CandidateView } from "@/generated/api-types";

export function CandidateComparisonPage({ runId }: { runId: string }) {
  const detail = useRunDetail(runId);
  const approveCommit = useApproveCommit();
  const [sorting, setSorting] = useState<SortingState>([{ id: "rank", desc: false }]);
  const [leftCandidate, setLeftCandidate] = useState<string | null>(null);
  const [rightCandidate, setRightCandidate] = useState<string | null>(null);
  const [note, setNote] = useState("");

  const candidates = useMemo(() => detail.data?.candidates ?? [], [detail.data]);
  const leftDiff = useCandidateDiff(runId, leftCandidate);
  const rightDiff = useCandidateDiff(runId, rightCandidate);

  const columns = useMemo<ColumnDef<CandidateView>[]>(
    () => [
      {
        id: "rank",
        header: "Rank",
        accessorFn: (row) => row.rank ?? Number.MAX_SAFE_INTEGER,
        cell: ({ row }) =>
          row.original.rank === null ? (
            <span className="text-[var(--text-muted)]">excluded</span>
          ) : (
            <span className="flex items-center gap-1.5 font-mono">
              {row.original.rank}
              {row.original.is_winner && (
                <Trophy aria-label="Winner" className="size-3.5 text-[var(--state-success)]" />
              )}
            </span>
          ),
      },
      {
        id: "candidate",
        header: "Candidate",
        accessorFn: (row) => row.candidate_id,
        cell: ({ row }) => (
          <span className="font-mono text-[var(--text-primary)]">{row.original.candidate_id}</span>
        ),
      },
      {
        id: "strategy",
        header: "Strategy",
        accessorFn: (row) => row.strategy_label,
      },
      {
        id: "status",
        header: "Status",
        accessorFn: (row) => row.status,
        cell: ({ row }) => (
          <Badge tone={candidateStatusTone(row.original.status)}>{row.original.status_label}</Badge>
        ),
      },
      {
        id: "tests",
        header: "Tests",
        accessorFn: (row) =>
          row.tests_passed === null ? "not run" : row.tests_passed ? "passed" : "failed",
      },
      {
        id: "review",
        header: "Review",
        accessorFn: (row) =>
          row.review_passed === null ? "not run" : row.review_passed ? "passed" : "failed",
      },
      {
        id: "coverage",
        header: "Coverage",
        accessorFn: (row) => row.line_coverage_percent ?? -1,
        cell: ({ row }) =>
          row.original.line_coverage_percent == null
            ? "not measured"
            : `${row.original.line_coverage_percent.toFixed(2)}%`,
      },
      {
        id: "changedLines",
        header: "Changed lines",
        accessorFn: (row) => row.changed_lines,
      },
      {
        id: "changedFiles",
        header: "Files",
        accessorFn: (row) => row.changed_files,
      },
      {
        id: "repairs",
        header: "Repairs",
        accessorFn: (row) => row.repairs_used,
        cell: ({ row }) =>
          `${String(row.original.repairs_used)}/${String(row.original.repair_budget)}`,
      },
      {
        id: "gateDuration",
        header: "Gate time",
        accessorFn: (row) => row.gate_duration,
        cell: ({ row }) => formatDuration(row.original.gate_duration),
      },
    ],
    [],
  );

  const table = useReactTable({
    data: candidates,
    columns,
    state: { sorting },
    onSortingChange: setSorting,
    getCoreRowModel: getCoreRowModel(),
    getSortedRowModel: getSortedRowModel(),
  });

  if (detail.isPending) {
    return <LoadingState label="Loading the candidate comparison" />;
  }

  if (detail.isError) {
    return (
      <div className="p-6">
        <ErrorState
          title="The comparison could not be loaded"
          message={detail.error.message}
          onRetry={() => {
            void detail.refetch();
          }}
        />
      </div>
    );
  }

  const run = detail.data;
  const winner = candidates.find((candidate) => candidate.is_winner) ?? null;
  const ineligible = candidates.filter((candidate) => candidate.exclusion_summaries.length > 0);
  const awaitingCommit = run.summary.status === "awaiting_commit_approval";

  return (
    <div className="mx-auto flex max-w-[1600px] flex-col gap-4 p-6">
      <header className="flex flex-wrap items-start justify-between gap-4">
        <div>
          <h1 className="text-xl font-semibold text-[var(--text-primary)]">Candidate comparison</h1>
          <p className="mt-1 text-sm text-[var(--text-muted)]">
            Ranking uses a deterministic lexicographic tuple. The candidate identifier is the final
            tie-break, so repeated evaluation selects the same winner.
          </p>
        </div>
        <Link
          to="/runs/$runId"
          params={{ runId }}
          className="inline-flex h-9 items-center rounded-[var(--radius-medium)] border border-[var(--border-subtle)] px-4 text-sm text-[var(--text-secondary)] hover:text-[var(--text-primary)]"
        >
          Back to the run
        </Link>
      </header>

      {winner !== null ? (
        <section
          aria-labelledby="winner-heading"
          className="surface-panel border-[color-mix(in_srgb,var(--state-success)_45%,transparent)] bg-[color-mix(in_srgb,var(--state-success)_8%,transparent)] p-4"
        >
          <div className="flex flex-wrap items-start justify-between gap-4">
            <div className="min-w-0">
              <h2 id="winner-heading" className="flex items-center gap-2 text-sm font-semibold">
                <Trophy aria-hidden className="size-4 text-[var(--state-success)]" />
                Winner {winner.candidate_id}
              </h2>
              <p className="mt-1 text-xs text-[var(--text-secondary)]">
                Final integration:{" "}
                {run.projection.integration.final_tests_passed === true
                  ? "tests passed"
                  : run.projection.integration.final_tests_passed === false
                    ? "tests failed"
                    : "tests not run yet"}
                {" · "}
                {run.projection.integration.final_review_passed === true
                  ? "review passed"
                  : run.projection.integration.final_review_passed === false
                    ? "review failed"
                    : "review not run yet"}
              </p>
              {run.projection.commit != null && (
                <p className="mt-1 font-mono text-xs text-[var(--text-muted)]">
                  {run.projection.commit.commit_hash.slice(0, 12)} on {run.projection.commit.branch}{" "}
                  by {run.projection.commit.author_name}
                </p>
              )}
            </div>
            {awaitingCommit && (
              <div className="flex flex-col items-end gap-2">
                <input
                  className={`${inputClasses} w-64`}
                  value={note}
                  onChange={(event) => {
                    setNote(event.target.value);
                  }}
                  aria-label="Commit approval note"
                  placeholder="Approval note, optional"
                />
                <Button
                  tone="primary"
                  busy={approveCommit.isPending}
                  icon={<GitCommitHorizontal aria-hidden className="size-4" />}
                  onClick={() => {
                    approveCommit.mutate({
                      runId,
                      note: note.trim().length > 0 ? note.trim() : null,
                    });
                  }}
                >
                  Approve the commit
                </Button>
              </div>
            )}
          </div>

          {run.ranking_rationale.length > 0 && (
            <ul className="mt-3 grid gap-1 text-xs text-[var(--text-secondary)] sm:grid-cols-2">
              {run.ranking_rationale.map((line) => (
                <li key={line}>{line}</li>
              ))}
            </ul>
          )}
        </section>
      ) : (
        <Panel title="No winner selected">
          <EmptyState
            title="No candidate satisfied every required gate"
            description="The join node records an exclusion reason for each candidate. Review them below and adjust the task or the quality profile."
          />
        </Panel>
      )}

      <Panel title="Ranking" bodyClassName="p-0">
        <div className="scrollbar-slim overflow-x-auto">
          <table className="w-full min-w-[960px] text-xs">
            <caption className="sr-only">
              Candidate ranking table. Activate a column heading to sort.
            </caption>
            <thead>
              {table.getHeaderGroups().map((headerGroup) => (
                <tr key={headerGroup.id} className="border-b border-[var(--border-subtle)]">
                  {headerGroup.headers.map((header) => {
                    const sorted = header.column.getIsSorted();
                    return (
                      <th
                        key={header.id}
                        scope="col"
                        aria-sort={
                          sorted === "asc" ? "ascending" : sorted === "desc" ? "descending" : "none"
                        }
                        className="px-3 py-2 text-left font-medium text-[var(--text-muted)]"
                      >
                        <button
                          type="button"
                          onClick={header.column.getToggleSortingHandler()}
                          className="inline-flex items-center gap-1 hover:text-[var(--text-primary)]"
                        >
                          {flexRender(header.column.columnDef.header, header.getContext())}
                          {sorted === "asc" ? (
                            <ArrowUp aria-hidden className="size-3" />
                          ) : sorted === "desc" ? (
                            <ArrowDown aria-hidden className="size-3" />
                          ) : (
                            <ArrowUpDown aria-hidden className="size-3 opacity-50" />
                          )}
                        </button>
                      </th>
                    );
                  })}
                </tr>
              ))}
            </thead>
            <tbody>
              {table.getRowModel().rows.map((row) => (
                <tr
                  key={row.id}
                  className={`border-b border-[var(--border-subtle)] ${
                    row.original.is_winner
                      ? "bg-[color-mix(in_srgb,var(--state-success)_8%,transparent)]"
                      : row.original.exclusion_summaries.length > 0
                        ? "bg-[color-mix(in_srgb,var(--state-failure)_6%,transparent)]"
                        : ""
                  }`}
                >
                  {row.getVisibleCells().map((cell) => (
                    <td key={cell.id} className="px-3 py-2 text-[var(--text-secondary)]">
                      {flexRender(cell.column.columnDef.cell, cell.getContext())}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </Panel>

      {ineligible.length > 0 && (
        <Panel title="Exclusion reasons">
          <ul className="flex flex-col gap-3">
            {ineligible.map((candidate) => (
              <li key={candidate.candidate_id}>
                <p className="font-mono text-xs text-[var(--text-primary)]">
                  {candidate.candidate_id}
                </p>
                <ul className="mt-1 list-disc space-y-0.5 pl-5 text-xs text-[var(--state-failure)]">
                  {candidate.exclusion_summaries.map((reason) => (
                    <li key={reason}>{reason}</li>
                  ))}
                </ul>
              </li>
            ))}
          </ul>
        </Panel>
      )}

      <Panel title="Side by side diff summary">
        <div className="grid gap-4 lg:grid-cols-2">
          {[
            {
              value: leftCandidate,
              set: setLeftCandidate,
              diff: leftDiff,
              label: "Left candidate",
            },
            {
              value: rightCandidate,
              set: setRightCandidate,
              diff: rightDiff,
              label: "Right candidate",
            },
          ].map((side) => (
            <div key={side.label} className="flex min-h-0 flex-col gap-2">
              <label className="flex items-center gap-2 text-xs text-[var(--text-muted)]">
                <span>{side.label}</span>
                <select
                  className={`${inputClasses} flex-1`}
                  value={side.value ?? ""}
                  onChange={(event) => {
                    side.set(event.target.value.length > 0 ? event.target.value : null);
                  }}
                >
                  <option value="">Select a candidate</option>
                  {candidates.map((candidate) => (
                    <option key={candidate.candidate_id} value={candidate.candidate_id}>
                      {candidate.candidate_id} · {candidate.strategy_label}
                    </option>
                  ))}
                </select>
              </label>
              <div className="h-[360px]">
                {side.value === null ? (
                  <EmptyState
                    title="No candidate selected"
                    description="Choose a candidate to display its patch against the baseline."
                  />
                ) : side.diff.data === undefined ? (
                  <LoadingState label="Loading the diff" />
                ) : (
                  <DiffViewer patch={side.diff.data} label={`Diff for ${side.value}`} />
                )}
              </div>
            </div>
          ))}
        </div>
      </Panel>
    </div>
  );
}
