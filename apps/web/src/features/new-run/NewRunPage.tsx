import { useMemo, useState } from "react";
import { useNavigate } from "@tanstack/react-router";
import { FileUp, Play, ShieldCheck } from "lucide-react";

import { useConfiguration, useCreateRun, useDoctor } from "@/api/queries";
import { Badge } from "@/components/Badge";
import { Button } from "@/components/Button";
import { Field, inputClasses, textAreaClasses } from "@/components/Field";
import { Panel } from "@/components/Panel";
import { SelectField } from "@/components/SelectField";
import { SwitchField } from "@/components/SwitchField";
import { ErrorState, LoadingState } from "@/components/StateViews";
import { useSession } from "@/app/sessionContext";
import type { CreateRunRequest } from "@/generated/api-types";

interface FormState {
  repositoryPath: string;
  taskMarkdown: string;
  candidateCount: number;
  parallelCandidates: number;
  repairBudget: number;
  commitPolicy: string;
  qualityProfile: string;
  minimumCoverage: string;
  includeDirty: boolean;
  agentDriver: string;
  agentModel: string;
  wallClockMinutes: number;
}

const DEFAULT_FORM: FormState = {
  repositoryPath: "",
  taskMarkdown: "",
  candidateCount: 3,
  parallelCandidates: 3,
  repairBudget: 3,
  commitPolicy: "manual",
  qualityProfile: "standard",
  minimumCoverage: "",
  includeDirty: false,
  agentDriver: "local",
  agentModel: "",
  wallClockMinutes: 180,
};

export function NewRunPage() {
  const configuration = useConfiguration();
  const createRun = useCreateRun();
  const navigate = useNavigate();
  const session = useSession();
  const [form, setForm] = useState<FormState>(DEFAULT_FORM);
  const [preflightRequested, setPreflightRequested] = useState(false);

  const doctor = useDoctor(
    form.repositoryPath.trim().length > 0 ? form.repositoryPath.trim() : null,
    preflightRequested && form.repositoryPath.trim().length > 0,
  );

  const errors = useMemo(() => validate(form), [form]);
  const canSubmit = Object.keys(errors).length === 0 && !createRun.isPending;

  const update = <K extends keyof FormState>(key: K, value: FormState[K]) => {
    setForm((current) => ({ ...current, [key]: value }));
  };

  const importTaskFile = async (file: File | undefined) => {
    if (file === undefined) {
      return;
    }
    const text = await file.text();
    update("taskMarkdown", text);
  };

  const submit = () => {
    const payload: CreateRunRequest = {
      repository_path: form.repositoryPath.trim(),
      task_markdown: form.taskMarkdown,
      candidate_count: form.candidateCount,
      max_parallel_candidates: form.parallelCandidates,
      max_repairs_per_candidate: form.repairBudget,
      commit_policy: form.commitPolicy as CreateRunRequest["commit_policy"],
      quality_profile: form.qualityProfile as CreateRunRequest["quality_profile"],
      minimum_line_coverage:
        form.minimumCoverage.trim().length > 0 ? Number(form.minimumCoverage) : null,
      include_dirty: form.includeDirty,
      agent_driver: form.agentDriver,
      agent_model: form.agentModel.trim().length > 0 ? form.agentModel.trim() : null,
      demonstration_mode: session.demonstrationMode || form.agentDriver === "fake",
      wall_clock_seconds: form.wallClockMinutes * 60,
    };
    createRun.mutate(payload, {
      onSuccess: (response) => {
        void navigate({ to: "/runs/$runId", params: { runId: response.run_id } });
      },
    });
  };

  if (configuration.isPending) {
    return <LoadingState label="Loading the run configuration" />;
  }

  if (configuration.isError) {
    return (
      <div className="p-6">
        <ErrorState
          title="The configuration could not be loaded"
          message={configuration.error.message}
          onRetry={() => {
            void configuration.refetch();
          }}
        />
      </div>
    );
  }

  const drivers = configuration.data.agent_drivers;

  return (
    <div className="mx-auto flex max-w-5xl flex-col gap-4 p-6">
      <div>
        <h1 className="text-xl font-semibold text-[var(--text-primary)]">New run</h1>
        <p className="mt-1 text-sm text-[var(--text-muted)]">
          Heikas Forge writes a plan first and waits for your approval before any candidate source
          is changed.
        </p>
      </div>

      <div className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_320px]">
        <div className="flex flex-col gap-4">
          <Panel title="Repository and task">
            <div className="flex flex-col gap-4">
              <Field
                label="Repository path"
                hint="An absolute path to a local Git working tree."
                error={errors["repositoryPath"] ?? null}
              >
                {(identifier, describedBy) => (
                  <input
                    id={identifier}
                    aria-describedby={describedBy}
                    className={inputClasses}
                    value={form.repositoryPath}
                    list="recent-repositories"
                    placeholder="/home/you/projects/example"
                    onChange={(event) => {
                      update("repositoryPath", event.target.value);
                    }}
                  />
                )}
              </Field>
              <datalist id="recent-repositories">
                {configuration.data.recent_repositories.map((path) => (
                  <option key={path} value={path} />
                ))}
              </datalist>

              <Field
                label="Task"
                hint="Describe the outcome you want. The first line becomes the run title."
                error={errors["taskMarkdown"] ?? null}
              >
                {(identifier, describedBy) => (
                  <textarea
                    id={identifier}
                    aria-describedby={describedBy}
                    className={textAreaClasses}
                    value={form.taskMarkdown}
                    placeholder={
                      "Fix the rounding defect in the invoice total\n\nThe total should round half away from zero."
                    }
                    onChange={(event) => {
                      update("taskMarkdown", event.target.value);
                    }}
                  />
                )}
              </Field>

              <label className="inline-flex w-fit cursor-pointer items-center gap-2 rounded-[var(--radius-medium)] border border-[var(--border-subtle)] px-3 py-2 text-sm text-[var(--text-secondary)] hover:text-[var(--text-primary)]">
                <FileUp aria-hidden className="size-4" />
                Import a task file
                <input
                  type="file"
                  accept=".md,.txt"
                  className="sr-only"
                  onChange={(event) => {
                    void importTaskFile(event.target.files?.[0]);
                  }}
                />
              </label>
            </div>
          </Panel>

          <Panel title="Candidates and budgets">
            <div className="grid gap-4 sm:grid-cols-2">
              <Field
                label="Candidate count"
                hint={`Between 1 and ${String(configuration.data.maximum_candidate_count)}.`}
                error={errors["candidateCount"] ?? null}
              >
                {(identifier, describedBy) => (
                  <input
                    id={identifier}
                    aria-describedby={describedBy}
                    type="number"
                    min={1}
                    max={configuration.data.maximum_candidate_count}
                    className={inputClasses}
                    value={form.candidateCount}
                    onChange={(event) => {
                      update("candidateCount", Number(event.target.value));
                    }}
                  />
                )}
              </Field>
              <Field
                label="Maximum parallel candidates"
                error={errors["parallelCandidates"] ?? null}
              >
                {(identifier, describedBy) => (
                  <input
                    id={identifier}
                    aria-describedby={describedBy}
                    type="number"
                    min={1}
                    max={configuration.data.maximum_candidate_count}
                    className={inputClasses}
                    value={form.parallelCandidates}
                    onChange={(event) => {
                      update("parallelCandidates", Number(event.target.value));
                    }}
                  />
                )}
              </Field>
              <Field
                label="Repair attempts for each candidate"
                error={errors["repairBudget"] ?? null}
              >
                {(identifier, describedBy) => (
                  <input
                    id={identifier}
                    aria-describedby={describedBy}
                    type="number"
                    min={0}
                    max={10}
                    className={inputClasses}
                    value={form.repairBudget}
                    onChange={(event) => {
                      update("repairBudget", Number(event.target.value));
                    }}
                  />
                )}
              </Field>
              <Field
                label="Wall clock budget in minutes"
                error={errors["wallClockMinutes"] ?? null}
              >
                {(identifier, describedBy) => (
                  <input
                    id={identifier}
                    aria-describedby={describedBy}
                    type="number"
                    min={5}
                    max={1440}
                    className={inputClasses}
                    value={form.wallClockMinutes}
                    onChange={(event) => {
                      update("wallClockMinutes", Number(event.target.value));
                    }}
                  />
                )}
              </Field>
            </div>
          </Panel>

          <Panel title="Agent, quality and Git policy">
            <div className="grid gap-4 sm:grid-cols-2">
              <SelectField
                label="Agent driver"
                value={form.agentDriver}
                onValueChange={(value) => {
                  update("agentDriver", value);
                }}
                options={drivers.map((driver) => ({
                  value: driver.id,
                  label: driver.label,
                  description: driver.requires_paid_account
                    ? "Optional adapter that needs its own account"
                    : driver.demonstration_only
                      ? "Deterministic fixture replay for demonstrations"
                      : "Free local path",
                  disabled: driver.demonstration_only && !session.demonstrationMode,
                }))}
                hint="The built-in local driver needs no paid account."
              />
              <Field label="Model identifier" hint="Optional override for the selected driver.">
                {(identifier, describedBy) => (
                  <input
                    id={identifier}
                    aria-describedby={describedBy}
                    className={inputClasses}
                    value={form.agentModel}
                    placeholder="qwen2.5-coder:14b"
                    onChange={(event) => {
                      update("agentModel", event.target.value);
                    }}
                  />
                )}
              </Field>
              <SelectField
                label="Quality profile"
                value={form.qualityProfile}
                onValueChange={(value) => {
                  update("qualityProfile", value);
                }}
                options={configuration.data.quality_profiles.map((profile) => ({
                  value: profile,
                  label: profile === "strict" ? "Strict" : "Standard",
                  description:
                    profile === "strict"
                      ? "Format, lint, audit, secret scan, static analysis and policy commands are all required"
                      : "A required lint command with the configured tests",
                }))}
              />
              <Field label="Minimum line coverage" hint="Leave empty to use the profile default.">
                {(identifier, describedBy) => (
                  <input
                    id={identifier}
                    aria-describedby={describedBy}
                    className={inputClasses}
                    inputMode="decimal"
                    value={form.minimumCoverage}
                    placeholder="80"
                    onChange={(event) => {
                      update("minimumCoverage", event.target.value);
                    }}
                  />
                )}
              </Field>
              <SelectField
                label="Commit policy"
                value={form.commitPolicy}
                onValueChange={(value) => {
                  update("commitPolicy", value);
                }}
                options={configuration.data.commit_policies.map((policy) => ({
                  value: policy,
                  label:
                    policy === "manual"
                      ? "Manual approval"
                      : policy === "automatic"
                        ? "Automatic after the final gates"
                        : "Do not commit",
                }))}
              />
              <SwitchField
                label="Include uncommitted changes"
                description="Capture the current working tree as the candidate baseline."
                checked={form.includeDirty}
                onCheckedChange={(checked) => {
                  update("includeDirty", checked);
                }}
              />
            </div>
          </Panel>
        </div>

        <aside className="flex flex-col gap-4">
          <Panel
            title="Preflight"
            actions={
              <Button
                tone="secondary"
                size="small"
                onClick={() => {
                  setPreflightRequested(true);
                  void doctor.refetch();
                }}
                disabled={form.repositoryPath.trim().length === 0}
                icon={<ShieldCheck aria-hidden className="size-4" />}
              >
                Check
              </Button>
            }
          >
            {!preflightRequested && (
              <p className="text-sm text-[var(--text-muted)]">
                Run the preflight check to validate Git, the agent driver and every configured
                command before the run starts.
              </p>
            )}
            {preflightRequested && doctor.isPending && (
              <LoadingState label="Checking the environment" />
            )}
            {preflightRequested && doctor.isError && (
              <ErrorState
                title="The preflight check failed"
                message={doctor.error.message}
                onRetry={() => {
                  void doctor.refetch();
                }}
              />
            )}
            {doctor.data !== undefined && (
              <ul className="flex flex-col gap-2">
                {doctor.data.checks.map((check) => (
                  <li key={check.id} className="flex items-start justify-between gap-2 text-xs">
                    <span className="text-[var(--text-secondary)]">{check.title}</span>
                    <Badge
                      tone={
                        check.outcome === "passed"
                          ? "success"
                          : check.outcome === "warning"
                            ? "warning"
                            : check.outcome === "failed"
                              ? "failure"
                              : "neutral"
                      }
                    >
                      {check.outcome}
                    </Badge>
                  </li>
                ))}
              </ul>
            )}
          </Panel>

          <Panel title="Ready to start">
            <div className="flex flex-col gap-3">
              {Object.keys(errors).length > 0 && (
                <p role="status" className="text-xs text-[var(--state-failure)]">
                  {Object.keys(errors).length === 1
                    ? "One field still needs attention before the run can start."
                    : `${String(Object.keys(errors).length)} fields still need attention before the run can start.`}
                </p>
              )}
              {createRun.isError && (
                <ErrorState
                  title="The run could not be created"
                  message={createRun.error.message}
                  sourceChangesPossible={false}
                />
              )}
              <Button
                tone="primary"
                onClick={submit}
                disabled={!canSubmit}
                busy={createRun.isPending}
                icon={<Play aria-hidden className="size-4" />}
              >
                Create run and plan
              </Button>
              <p className="text-xs text-[var(--text-muted)]">
                Planning is read-only. No candidate worktree is created until you approve the plan.
              </p>
            </div>
          </Panel>
        </aside>
      </div>
    </div>
  );
}

function validate(form: FormState): Record<string, string> {
  const errors: Record<string, string> = {};
  if (form.repositoryPath.trim().length === 0) {
    errors["repositoryPath"] = "Supply the path to a local Git repository.";
  }
  if (form.taskMarkdown.trim().length < 12) {
    errors["taskMarkdown"] =
      "Describe the task in at least a sentence so the plan can be specific.";
  }
  if (form.candidateCount < 1 || form.candidateCount > 8) {
    errors["candidateCount"] = "Choose between 1 and 8 candidates.";
  }
  if (form.parallelCandidates < 1 || form.parallelCandidates > form.candidateCount) {
    errors["parallelCandidates"] = "Parallel candidates cannot exceed the candidate count.";
  }
  if (form.repairBudget < 0 || form.repairBudget > 10) {
    errors["repairBudget"] = "Allow between 0 and 10 repair attempts.";
  }
  if (form.wallClockMinutes < 5 || form.wallClockMinutes > 1440) {
    errors["wallClockMinutes"] = "Choose a wall clock budget between 5 and 1440 minutes.";
  }
  return errors;
}
