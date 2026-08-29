import { useEffect, useMemo, useState } from "react";
import { Link, useNavigate } from "@tanstack/react-router";
import { CheckCircle2, Eye, PencilLine, RotateCcw, ShieldOff } from "lucide-react";

import {
  useApprovePlan,
  usePlan,
  useRejectPlan,
  useRevisePlan,
  useUpdatePlan,
} from "@/api/queries";
import { Badge } from "@/components/Badge";
import { Button } from "@/components/Button";
import { CodeEditor } from "@/components/CodeEditor";
import { Dialog } from "@/components/Dialog";
import { MarkdownView } from "@/components/MarkdownView";
import { Panel } from "@/components/Panel";
import { EmptyState, ErrorState, LoadingState } from "@/components/StateViews";
import { inputClasses, textAreaClasses } from "@/components/Field";
import { formatTimestamp } from "@/app/format";

export function PlanPage({ runId }: { runId: string }) {
  const plan = usePlan(runId);
  const approvePlan = useApprovePlan();
  const updatePlan = useUpdatePlan();
  const revisePlan = useRevisePlan();
  const rejectPlan = useRejectPlan();
  const navigate = useNavigate();

  const [mode, setMode] = useState<"read" | "edit">("read");
  const [draft, setDraft] = useState("");
  const [note, setNote] = useState("");
  const [selectedVersion, setSelectedVersion] = useState<number | null>(null);
  const [reviseOpen, setReviseOpen] = useState(false);
  const [reviseNote, setReviseNote] = useState("");
  const [rejectOpen, setRejectOpen] = useState(false);
  const [rejectReason, setRejectReason] = useState("");

  useEffect(() => {
    if (plan.data?.markdown != null) {
      setDraft(plan.data.markdown);
      setSelectedVersion(plan.data.version);
    }
  }, [plan.data?.markdown, plan.data?.version]);

  const dirty = useMemo(
    () => plan.data?.markdown != null && draft !== plan.data.markdown,
    [draft, plan.data?.markdown],
  );

  if (plan.isPending) {
    return <LoadingState label="Loading the plan" />;
  }

  if (plan.isError) {
    return (
      <div className="p-6">
        <ErrorState
          title="The plan could not be loaded"
          message={plan.error.message}
          onRetry={() => {
            void plan.refetch();
          }}
        />
      </div>
    );
  }

  if (plan.data.markdown === null) {
    return (
      <div className="p-6">
        <Panel title="No plan version yet">
          <EmptyState
            title="The planning node has not produced a plan"
            description="The read-only planning node inspects the repository before it writes the first plan version."
            action={
              <Link
                to="/runs/$runId"
                params={{ runId }}
                className="inline-flex h-9 items-center rounded-[var(--radius-medium)] border border-[var(--border-subtle)] px-4 text-sm text-[var(--text-secondary)]"
              >
                Back to the run
              </Link>
            }
          />
        </Panel>
      </div>
    );
  }

  const validation = plan.data.validation;
  const missingHeadings = validation?.missing_headings ?? [];
  const expectedFiles = validation?.expected_files ?? [];
  const approval = plan.data.history.approval;
  const locked = plan.data.candidate_work_started;

  return (
    <div className="mx-auto flex max-w-[1500px] flex-col gap-4 p-6">
      <header className="flex flex-wrap items-start justify-between gap-4">
        <div>
          <h1 className="text-xl font-semibold text-[var(--text-primary)]">Plan approval</h1>
          <p className="mt-1 max-w-2xl text-sm text-[var(--text-muted)]">
            No candidate source has been changed yet. Approving records the exact plan hash, and any
            later edit invalidates that approval automatically.
          </p>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <Badge tone={plan.data.approved ? "success" : "warning"}>
            {plan.data.approved ? "Approved" : "Awaiting approval"}
          </Badge>
          <Badge tone="neutral" title="The BLAKE3 hash of the current plan version">
            v{plan.data.version} ·{" "}
            {plan.data.history.versions.at(-1)?.hash.slice(0, 12) ?? "unknown"}
          </Badge>
        </div>
      </header>

      {locked && (
        <p className="rounded-[var(--radius-medium)] border border-[color-mix(in_srgb,var(--state-warning)_40%,transparent)] bg-[var(--state-warning-surface)] px-3 py-2 text-xs text-[var(--state-warning)]">
          Candidate work has started, so the plan can no longer be edited or approved again.
        </p>
      )}

      <div className="grid gap-4 xl:grid-cols-[minmax(0,1fr)_340px]">
        <Panel
          title={mode === "read" ? "Plan" : "Editing the plan"}
          description={
            mode === "edit"
              ? "Saving creates a new plan version and invalidates any earlier approval."
              : undefined
          }
          actions={
            <div className="flex items-center gap-2">
              <Button
                tone={mode === "read" ? "primary" : "ghost"}
                size="small"
                onClick={() => {
                  setMode("read");
                }}
                icon={<Eye aria-hidden className="size-3.5" />}
              >
                Read
              </Button>
              <Button
                tone={mode === "edit" ? "primary" : "ghost"}
                size="small"
                disabled={locked}
                onClick={() => {
                  setMode("edit");
                }}
                icon={<PencilLine aria-hidden className="size-3.5" />}
              >
                Edit
              </Button>
            </div>
          }
          bodyClassName="min-h-0"
        >
          {mode === "read" ? (
            <div className="scrollbar-slim max-h-[62vh] overflow-auto pr-2">
              <MarkdownView markdown={draft} />
            </div>
          ) : (
            <div className="flex flex-col gap-3">
              <div className="h-[58vh] overflow-hidden rounded-[var(--radius-medium)] border border-[var(--border-subtle)]">
                <CodeEditor
                  value={draft}
                  onChange={setDraft}
                  label="Plan markdown editor"
                  className="h-full"
                />
              </div>
              <div className="flex items-center gap-2">
                <Button
                  tone="secondary"
                  busy={updatePlan.isPending}
                  disabled={!dirty || locked}
                  onClick={() => {
                    updatePlan.mutate(
                      { runId, markdown: draft },
                      {
                        onSuccess: () => {
                          setMode("read");
                        },
                      },
                    );
                  }}
                >
                  Save as a new version
                </Button>
                <Button
                  tone="ghost"
                  disabled={!dirty}
                  onClick={() => {
                    setDraft(plan.data.markdown ?? "");
                  }}
                  icon={<RotateCcw aria-hidden className="size-3.5" />}
                >
                  Discard changes
                </Button>
                {dirty && (
                  <span className="text-xs text-[var(--state-warning)]">
                    Unsaved changes will not be approved until you save them.
                  </span>
                )}
              </div>
            </div>
          )}
        </Panel>

        <aside className="flex flex-col gap-4">
          <Panel title="Repository findings">
            {expectedFiles.length === 0 ? (
              <p className="text-xs text-[var(--text-muted)]">
                The plan lists no expected file changes.
              </p>
            ) : (
              <ul className="flex flex-col gap-1 text-xs">
                {expectedFiles.map((file) => (
                  <li key={file} className="truncate font-mono text-[var(--text-secondary)]">
                    {file}
                  </li>
                ))}
              </ul>
            )}
            {missingHeadings.length > 0 && (
              <p className="mt-3 text-xs text-[var(--state-failure)]">
                Missing required headings: {missingHeadings.join(", ")}
              </p>
            )}
          </Panel>

          <Panel title="Version history">
            <ul className="flex flex-col gap-2 text-xs">
              {plan.data.history.versions.map((version) => (
                <li key={version.version}>
                  <button
                    type="button"
                    aria-pressed={selectedVersion === version.version}
                    onClick={() => {
                      setSelectedVersion(version.version);
                    }}
                    className={`flex w-full flex-col gap-0.5 rounded-[var(--radius-medium)] border px-3 py-2 text-left ${
                      selectedVersion === version.version
                        ? "border-[var(--accent-primary)]"
                        : "border-[var(--border-subtle)]"
                    }`}
                  >
                    <span className="flex items-center justify-between gap-2">
                      <span className="font-medium text-[var(--text-primary)]">
                        Version {version.version}
                      </span>
                      <Badge tone={version.author === "human" ? "info" : "neutral"}>
                        {version.author === "human" ? "Edited by you" : "Written by the agent"}
                      </Badge>
                    </span>
                    <span className="text-[var(--text-muted)]">
                      {formatTimestamp(version.created_at)} · {version.byte_length} bytes
                    </span>
                    <span className="truncate font-mono text-[var(--text-muted)]">
                      {version.hash.slice(0, 24)}
                    </span>
                    {version.revision_note !== null && (
                      <span className="text-[var(--text-secondary)]">{version.revision_note}</span>
                    )}
                  </button>
                </li>
              ))}
            </ul>
          </Panel>

          <Panel title="Decision">
            <div className="flex flex-col gap-3">
              {approval !== null && (
                <p className="text-xs text-[var(--text-muted)]">
                  Last decision: {approval.decision.replace(/_/g, " ")} by {approval.local_user} on{" "}
                  {formatTimestamp(approval.decided_at)}.
                </p>
              )}
              <label className="flex flex-col gap-1.5 text-xs text-[var(--text-secondary)]">
                Approval note, optional
                <input
                  className={inputClasses}
                  value={note}
                  onChange={(event) => {
                    setNote(event.target.value);
                  }}
                  placeholder="Looks correct, proceed"
                />
              </label>
              {approvePlan.isError && (
                <ErrorState
                  title="The plan could not be approved"
                  message={approvePlan.error.message}
                  sourceChangesPossible={false}
                />
              )}
              <Button
                tone="primary"
                disabled={locked || missingHeadings.length > 0}
                busy={approvePlan.isPending}
                icon={<CheckCircle2 aria-hidden className="size-4" />}
                onClick={() => {
                  approvePlan.mutate(
                    {
                      runId,
                      markdown: dirty ? draft : null,
                      note: note.trim().length > 0 ? note.trim() : null,
                    },
                    {
                      onSuccess: () => {
                        void navigate({ to: "/runs/$runId", params: { runId } });
                      },
                    },
                  );
                }}
              >
                Approve and start candidates
              </Button>
              <Button
                tone="secondary"
                disabled={locked}
                onClick={() => {
                  setReviseOpen(true);
                }}
                icon={<RotateCcw aria-hidden className="size-4" />}
              >
                Request a revision
              </Button>
              <Button
                tone="danger"
                disabled={locked}
                onClick={() => {
                  setRejectOpen(true);
                }}
                icon={<ShieldOff aria-hidden className="size-4" />}
              >
                Reject the run
              </Button>
              <p className="text-xs text-[var(--text-muted)]">
                Rejecting ends the run before any candidate worktree is created.
              </p>
            </div>
          </Panel>
        </aside>
      </div>

      <Dialog
        open={reviseOpen}
        onOpenChange={setReviseOpen}
        title="Request a new plan version"
        description="The planning node runs again with your note and the previous plan as evidence."
        footer={
          <>
            <Button
              tone="ghost"
              onClick={() => {
                setReviseOpen(false);
              }}
            >
              Cancel
            </Button>
            <Button
              tone="primary"
              busy={revisePlan.isPending}
              disabled={reviseNote.trim().length === 0}
              onClick={() => {
                revisePlan.mutate(
                  { runId, note: reviseNote.trim() },
                  {
                    onSuccess: () => {
                      setReviseOpen(false);
                      setReviseNote("");
                    },
                  },
                );
              }}
            >
              Request the revision
            </Button>
          </>
        }
      >
        <label className="flex flex-col gap-1.5 text-xs text-[var(--text-secondary)]">
          What should change
          <textarea
            className={textAreaClasses}
            value={reviseNote}
            onChange={(event) => {
              setReviseNote(event.target.value);
            }}
            placeholder="Cover the boundary case where the invoice total is exactly one half."
          />
        </label>
      </Dialog>

      <Dialog
        open={rejectOpen}
        onOpenChange={setRejectOpen}
        title="Reject this plan"
        description="The run ends immediately. No candidate worktree is created and your repository is untouched."
        footer={
          <>
            <Button
              tone="ghost"
              onClick={() => {
                setRejectOpen(false);
              }}
            >
              Keep the run
            </Button>
            <Button
              tone="danger"
              busy={rejectPlan.isPending}
              onClick={() => {
                rejectPlan.mutate(
                  {
                    runId,
                    reason: rejectReason.trim().length > 0 ? rejectReason.trim() : null,
                  },
                  {
                    onSuccess: () => {
                      setRejectOpen(false);
                    },
                  },
                );
              }}
            >
              Reject the run
            </Button>
          </>
        }
      >
        <label className="flex flex-col gap-1.5 text-xs text-[var(--text-secondary)]">
          Reason, optional
          <input
            className={inputClasses}
            value={rejectReason}
            onChange={(event) => {
              setRejectReason(event.target.value);
            }}
            placeholder="The task description was wrong"
          />
        </label>
      </Dialog>
    </div>
  );
}
