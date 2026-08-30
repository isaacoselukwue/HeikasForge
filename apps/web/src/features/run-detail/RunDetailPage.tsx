import { useMemo, useState } from "react";
import { Link } from "@tanstack/react-router";
import {
  Ban,
  Download,
  FileCheck2,
  GitCommitHorizontal,
  Play,
  RadioTower,
  Scale,
} from "lucide-react";

import {
  useCancelRun,
  useExportRun,
  useIntegrationDiff,
  useResumeRun,
  useRunDetail,
} from "@/api/queries";
import { useRunEventStream } from "@/api/stream";
import { Badge } from "@/components/Badge";
import { candidateStatusTone, runStatusTone } from "@/components/tone";
import { Button } from "@/components/Button";
import { Dialog } from "@/components/Dialog";
import { DiffViewer } from "@/components/DiffViewer";
import { LiveRegion } from "@/components/LiveRegion";
import { Panel } from "@/components/Panel";
import { Tabs } from "@/components/Tabs";
import { ErrorState, LoadingState } from "@/components/StateViews";
import { inputClasses } from "@/components/Field";
import { formatDuration, formatRelative, shortRunId } from "@/app/format";
import { CandidateInspector } from "./CandidateInspector";
import { RunGraph } from "./RunGraph";
import { RunLogs } from "./RunLogs";
import { RunTimeline } from "./RunTimeline";

const TERMINAL = ["succeeded", "failed", "cancelled", "exhausted"];

export function RunDetailPage({ runId }: { runId: string }) {
  const detail = useRunDetail(runId);
  const cancelRun = useCancelRun();
  const resumeRun = useResumeRun();
  const exportRun = useExportRun();
  const [tab, setTab] = useState("graph");
  const [candidateFilter, setCandidateFilter] = useState("all");
  const [selectedCandidate, setSelectedCandidate] = useState<string | null>(null);
  const [cancelOpen, setCancelOpen] = useState(false);
  const [cancelReason, setCancelReason] = useState("");

  const stream = useRunEventStream(runId, 0);
  const integrationDiff = useIntegrationDiff(
    runId,
    detail.data?.projection.integration.applied_candidate != null,
  );

  const candidates = useMemo(() => detail.data?.candidates ?? [], [detail.data]);
  const selected = useMemo(
    () =>
      candidates.find((candidate) => candidate.candidate_id === selectedCandidate) ??
      candidates.find((candidate) => candidate.is_winner) ??
      candidates[0] ??
      null,
    [candidates, selectedCandidate],
  );

  if (detail.isPending) {
    return <LoadingState label="Loading the run" />;
  }

  if (detail.isError) {
    return (
      <div className="p-6">
        <ErrorState
          title="The run could not be loaded"
          message={detail.error.message}
          remedy="Confirm the run identifier and that the orchestrator is running."
          onRetry={() => {
            void detail.refetch();
          }}
        />
      </div>
    );
  }

  const run = detail.data;
  const summary = run.summary;
  const isTerminal = TERMINAL.includes(summary.status);
  const needsPlanApproval = summary.status === "awaiting_plan_approval";
  const needsCommitApproval = summary.status === "awaiting_commit_approval";

  return (
    <div className="mx-auto flex max-w-[1600px] flex-col gap-4 p-6">
      <LiveRegion message={summary.last_event_summary ?? null} />

      <header className="flex flex-wrap items-start justify-between gap-4">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-2">
            <h1 className="truncate text-xl font-semibold text-[var(--text-primary)]">
              {summary.task_title}
            </h1>
            <Badge tone={runStatusTone(summary.status)}>{summary.status_label}</Badge>
            {summary.demonstration_mode && <Badge tone="warning">Demonstration mode</Badge>}
            <Badge
              tone={
                stream.state === "live"
                  ? "success"
                  : stream.state === "closed"
                    ? "neutral"
                    : "warning"
              }
              icon={<RadioTower aria-hidden className="size-3" />}
              title={`Live event stream is ${stream.state}. Last sequence ${String(stream.lastSequence)}.`}
            >
              {stream.state}
            </Badge>
          </div>
          <p className="mt-1 truncate text-sm text-[var(--text-muted)]">
            <span className="font-mono">{shortRunId(summary.run_id)}</span> ·{" "}
            {summary.repository_path} · elapsed {formatDuration(summary.elapsed)} · updated{" "}
            {formatRelative(summary.updated_at)}
          </p>
        </div>

        <div className="flex flex-wrap items-center gap-2">
          {needsPlanApproval && (
            <Link
              to="/runs/$runId/plan"
              params={{ runId }}
              className="inline-flex h-9 items-center gap-2 rounded-[var(--radius-medium)] bg-[var(--accent-primary-strong)] px-4 text-sm font-medium text-[var(--text-inverted)]"
            >
              <FileCheck2 aria-hidden className="size-4" />
              Review the plan
            </Link>
          )}
          {needsCommitApproval && (
            <Link
              to="/runs/$runId/candidates"
              params={{ runId }}
              className="inline-flex h-9 items-center gap-2 rounded-[var(--radius-medium)] bg-[var(--accent-primary-strong)] px-4 text-sm font-medium text-[var(--text-inverted)]"
            >
              <GitCommitHorizontal aria-hidden className="size-4" />
              Approve the commit
            </Link>
          )}
          {!isTerminal && (
            <>
              <Button
                tone="secondary"
                icon={<Play aria-hidden className="size-4" />}
                busy={resumeRun.isPending}
                onClick={() => {
                  resumeRun.mutate({ runId });
                }}
              >
                Resume
              </Button>
              <Button
                tone="danger"
                icon={<Ban aria-hidden className="size-4" />}
                onClick={() => {
                  setCancelOpen(true);
                }}
              >
                Cancel
              </Button>
            </>
          )}
          <Button
            tone="secondary"
            icon={<Download aria-hidden className="size-4" />}
            busy={exportRun.isPending}
            onClick={() => {
              exportRun.mutate({ runId, includeWorktrees: false });
            }}
          >
            Export evidence
          </Button>
          <Link
            to="/runs/$runId/candidates"
            params={{ runId }}
            className="inline-flex h-9 items-center gap-2 rounded-[var(--radius-medium)] border border-[var(--border-subtle)] px-4 text-sm text-[var(--text-secondary)] hover:text-[var(--text-primary)]"
          >
            <Scale aria-hidden className="size-4" />
            Compare candidates
          </Link>
        </div>
      </header>

      {summary.recovery_reason != null && (
        <ErrorState
          title="This run requires recovery"
          message={summary.recovery_reason}
          remedy="Export the evidence before attempting a repair, then resume the run."
          lastDurableEvent={summary.last_event_summary ?? null}
          sourceChangesPossible={candidates.length > 0}
        />
      )}

      {exportRun.isSuccess && (
        <p
          className={
            exportRun.data.redacted
              ? "rounded-[var(--radius-medium)] border border-[var(--border-subtle)] bg-[var(--state-success-surface)] px-3 py-2 text-xs text-[var(--state-success)]"
              : "rounded-[var(--radius-medium)] border border-[var(--border-subtle)] bg-[var(--state-warning-surface)] px-3 py-2 text-xs text-[var(--state-warning)]"
          }
        >
          {exportRun.data.redacted
            ? `Evidence archive written to ${exportRun.data.archive_path}. All ${String(exportRun.data.redacted_entries)} entries were redacted.`
            : `Evidence archive written to ${exportRun.data.archive_path}. ${String(exportRun.data.unredactable_entries)} entries are not text and were archived without redaction.`}
          {exportRun.data.excluded_sensitive_paths.length > 0
            ? ` ${String(exportRun.data.excluded_sensitive_paths.length)} paths matching a sensitive pattern were excluded.`
            : ""}
        </p>
      )}

      <div className="grid gap-4 xl:grid-cols-[minmax(0,1fr)_360px]">
        <Panel title="Execution" bodyClassName="flex min-h-0 flex-col gap-3">
          <Tabs
            label="Run views"
            value={tab}
            onValueChange={setTab}
            className="min-h-0 flex-1"
            tabs={[
              {
                value: "graph",
                label: "Graph",
                content: (
                  <div className="pt-3">
                    <RunGraph nodes={run.graph.nodes} edges={run.graph.edges} />
                    <p className="mt-2 text-xs text-[var(--text-muted)]">
                      Executed edges are highlighted. Use the graph controls to pan, zoom and fit
                      the run.
                    </p>
                  </div>
                ),
              },
              {
                value: "timeline",
                label: "Timeline",
                content: (
                  <div className="pt-3">
                    <RunTimeline
                      entries={run.timeline}
                      candidateFilter={candidateFilter}
                      onCandidateFilterChange={setCandidateFilter}
                      candidates={candidates.map((candidate) => candidate.candidate_id)}
                    />
                  </div>
                ),
              },
              {
                value: "logs",
                label: "Logs",
                content: (
                  <div className="pt-3">
                    <RunLogs runId={runId} />
                  </div>
                ),
              },
              {
                value: "integration",
                label: "Integration diff",
                content: (
                  <div className="h-[440px] pt-3">
                    {integrationDiff.data === undefined ? (
                      <LoadingState label="Loading the integration diff" />
                    ) : (
                      <DiffViewer patch={integrationDiff.data} label="Integration worktree diff" />
                    )}
                  </div>
                ),
              },
            ]}
          />
        </Panel>

        <div className="flex flex-col gap-4">
          <Panel title="Run metrics">
            <dl className="grid grid-cols-2 gap-3 text-xs">
              <Metric label="Node executions" value={String(run.metrics.node_executions)} />
              <Metric label="Node failures" value={String(run.metrics.node_failures)} />
              <Metric label="Repair loops" value={String(run.metrics.repair_loops)} />
              <Metric label="Automatic retries" value={String(run.metrics.automatic_retries)} />
              <Metric label="Test time" value={formatDuration(run.metrics.test_duration_ms)} />
              <Metric label="Review time" value={formatDuration(run.metrics.review_duration_ms)} />
              <Metric label="Agent time" value={formatDuration(run.metrics.agent_duration_ms)} />
              <Metric label="Changed lines" value={String(run.metrics.changed_lines)} />
              <Metric
                label="Processes supervised"
                value={String(run.metrics.processes_supervised)}
              />
              <Metric label="Processes timed out" value={String(run.metrics.processes_timed_out)} />
              <Metric label="Events recorded" value={String(run.metrics.events_recorded)} />
              <Metric
                label="Reported cost"
                value={
                  run.metrics.reported_cost_minor_units == null
                    ? "not reported"
                    : `${String(run.metrics.reported_cost_minor_units / 100)} ${run.metrics.reported_cost_currency ?? ""}`
                }
              />
            </dl>
          </Panel>

          <Panel title="Candidates" description={`${String(candidates.length)} registered`}>
            <ul className="flex flex-col gap-2">
              {candidates.map((candidate) => (
                <li key={candidate.candidate_id}>
                  <button
                    type="button"
                    onClick={() => {
                      setSelectedCandidate(candidate.candidate_id);
                    }}
                    aria-pressed={selected?.candidate_id === candidate.candidate_id}
                    className={`flex w-full items-center justify-between gap-2 rounded-[var(--radius-medium)] border px-3 py-2 text-left text-xs transition-colors ${
                      selected?.candidate_id === candidate.candidate_id
                        ? "border-[var(--accent-primary)] bg-[color-mix(in_srgb,var(--accent-primary)_10%,transparent)]"
                        : "border-[var(--border-subtle)] hover:border-[var(--border-strong)]"
                    }`}
                  >
                    <span className="min-w-0">
                      <span className="block truncate font-mono text-[var(--text-primary)]">
                        {candidate.candidate_id}
                      </span>
                      <span className="block truncate text-[var(--text-muted)]">
                        {candidate.strategy_label} · {candidate.changed_lines} lines ·{" "}
                        {candidate.repairs_used}/{candidate.repair_budget} repairs
                      </span>
                    </span>
                    <span className="flex shrink-0 items-center gap-1">
                      {candidate.is_winner && <Badge tone="success">Winner</Badge>}
                      <Badge tone={candidateStatusTone(candidate.status)}>
                        {candidate.status_label}
                      </Badge>
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          </Panel>

          <Panel title="Selected candidate" bodyClassName="flex min-h-0 flex-col">
            <CandidateInspector runId={runId} candidate={selected} />
          </Panel>
        </div>
      </div>

      <Dialog
        open={cancelOpen}
        onOpenChange={setCancelOpen}
        title="Cancel this run"
        description="Cancellation terminates every candidate process tree. Recorded evidence is preserved."
        footer={
          <>
            <Button
              tone="ghost"
              onClick={() => {
                setCancelOpen(false);
              }}
            >
              Keep running
            </Button>
            <Button
              tone="danger"
              busy={cancelRun.isPending}
              onClick={() => {
                cancelRun.mutate(
                  { runId, reason: cancelReason.trim().length > 0 ? cancelReason.trim() : null },
                  {
                    onSuccess: () => {
                      setCancelOpen(false);
                    },
                  },
                );
              }}
            >
              Cancel the run
            </Button>
          </>
        }
      >
        <label className="flex flex-col gap-1.5 text-xs text-[var(--text-secondary)]">
          Reason, recorded in the durable event log
          <input
            className={inputClasses}
            value={cancelReason}
            onChange={(event) => {
              setCancelReason(event.target.value);
            }}
            placeholder="The task changed"
          />
        </label>
      </Dialog>
    </div>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <dt className="text-[var(--text-muted)]">{label}</dt>
      <dd className="mt-0.5 font-mono text-[var(--text-primary)]">{value}</dd>
    </div>
  );
}
