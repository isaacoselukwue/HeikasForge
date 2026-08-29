import { useState } from "react";

import { useCandidateDiff } from "@/api/queries";
import { Badge } from "@/components/Badge";
import { candidateStatusTone } from "@/components/tone";
import { DiffViewer } from "@/components/DiffViewer";
import { EmptyState, ErrorState, LoadingState } from "@/components/StateViews";
import { Tabs } from "@/components/Tabs";
import { formatDuration } from "@/app/format";
import type { CandidateView } from "@/generated/api-types";

export interface CandidateInspectorProps {
  runId: string;
  candidate: CandidateView | null;
}

export function CandidateInspector({ runId, candidate }: CandidateInspectorProps) {
  const [tab, setTab] = useState("summary");
  const diff = useCandidateDiff(runId, candidate?.candidate_id ?? null);

  if (candidate === null) {
    return (
      <EmptyState
        title="Select a candidate"
        description="Choose a candidate card to inspect its gates, score tuple and diff."
      />
    );
  }

  return (
    <Tabs
      label="Candidate inspector"
      value={tab}
      onValueChange={setTab}
      className="min-h-0 flex-1"
      tabs={[
        {
          value: "summary",
          label: "Summary",
          content: (
            <div className="flex flex-col gap-4 p-1">
              <div className="flex flex-wrap items-center gap-2">
                <Badge tone={candidateStatusTone(candidate.status)}>{candidate.status_label}</Badge>
                <Badge tone="neutral">{candidate.strategy_label}</Badge>
                {candidate.is_winner && <Badge tone="success">Selected winner</Badge>}
                {candidate.rank !== null && <Badge tone="accent">Rank {candidate.rank}</Badge>}
              </div>

              <dl className="grid grid-cols-2 gap-3 text-xs sm:grid-cols-3">
                <Metric label="Branch" value={candidate.branch} mono />
                <Metric
                  label="Repairs"
                  value={`${String(candidate.repairs_used)} of ${String(candidate.repair_budget)}`}
                />
                <Metric label="Changed files" value={String(candidate.changed_files)} />
                <Metric label="Changed lines" value={String(candidate.changed_lines)} />
                <Metric label="Gate duration" value={formatDuration(candidate.gate_duration)} />
                <Metric
                  label="Line coverage"
                  value={
                    candidate.line_coverage_percent == null
                      ? "not measured"
                      : `${candidate.line_coverage_percent.toFixed(2)}%`
                  }
                />
                <Metric
                  label="Tests"
                  value={
                    candidate.tests_passed == null
                      ? "not run"
                      : candidate.tests_passed
                        ? "passed"
                        : "failed"
                  }
                />
                <Metric
                  label="Review"
                  value={
                    candidate.review_passed == null
                      ? "not run"
                      : candidate.review_passed
                        ? "passed"
                        : "failed"
                  }
                />
                <Metric label="Promotable" value={candidate.promotable ? "yes" : "no"} />
              </dl>

              {candidate.exclusion_summaries.length > 0 && (
                <section aria-labelledby="exclusion-heading">
                  <h3
                    id="exclusion-heading"
                    className="text-xs font-semibold text-[var(--state-failure)]"
                  >
                    Why this candidate is ineligible
                  </h3>
                  <ul className="mt-1.5 list-disc space-y-1 pl-5 text-xs text-[var(--text-secondary)]">
                    {candidate.exclusion_summaries.map((reason) => (
                      <li key={reason}>{reason}</li>
                    ))}
                  </ul>
                </section>
              )}
            </div>
          ),
        },
        {
          value: "score",
          label: "Score tuple",
          content:
            candidate.score_components.length === 0 ? (
              <EmptyState
                title="No score recorded"
                description="A score tuple is computed only for candidates that satisfied every required gate."
              />
            ) : (
              <table className="w-full text-xs">
                <caption className="sr-only">
                  Deterministic score tuple, compared in order from the top
                </caption>
                <thead>
                  <tr className="text-left text-[var(--text-muted)]">
                    <th scope="col" className="py-2 pl-1 font-medium">
                      Order
                    </th>
                    <th scope="col" className="py-2 font-medium">
                      Component
                    </th>
                    <th scope="col" className="py-2 font-medium">
                      Value
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {candidate.score_components.map((component, index) => (
                    <tr key={component.label} className="border-t border-[var(--border-subtle)]">
                      <td className="py-1.5 pl-1 font-mono text-[var(--text-muted)]">
                        {index + 1}
                      </td>
                      <td className="py-1.5 text-[var(--text-secondary)]">{component.label}</td>
                      <td className="py-1.5 font-mono text-[var(--text-primary)]">
                        {component.value}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            ),
        },
        {
          value: "diff",
          label: "Diff",
          content: (
            <div className="flex min-h-0 flex-col p-1">
              {diff.isPending && <LoadingState label="Loading the candidate diff" />}
              {diff.isError && (
                <ErrorState
                  title="The diff could not be loaded"
                  message={diff.error.message}
                  onRetry={() => {
                    void diff.refetch();
                  }}
                />
              )}
              {diff.data !== undefined && (
                <div className="h-[420px]">
                  <DiffViewer patch={diff.data} label={`Diff for ${candidate.candidate_id}`} />
                </div>
              )}
            </div>
          ),
        },
      ]}
    />
  );
}

function Metric({ label, value, mono }: { label: string; value: string; mono?: boolean }) {
  return (
    <div>
      <dt className="text-[var(--text-muted)]">{label}</dt>
      <dd
        className={`mt-0.5 text-[var(--text-primary)] ${mono === true ? "truncate font-mono" : ""}`}
        title={mono === true ? value : undefined}
      >
        {value}
      </dd>
    </div>
  );
}
