import { useState } from "react";
import { Stethoscope } from "lucide-react";

import { useConfiguration, useDoctor } from "@/api/queries";
import { Badge } from "@/components/Badge";
import { Button } from "@/components/Button";
import { Field, inputClasses } from "@/components/Field";
import { Panel } from "@/components/Panel";
import { EmptyState, ErrorState, LoadingState } from "@/components/StateViews";
import type { CheckOutcome } from "@/generated/api-types";

const OUTCOME_TONE: Record<CheckOutcome, "success" | "warning" | "failure" | "neutral"> = {
  passed: "success",
  warning: "warning",
  failed: "failure",
  skipped: "neutral",
};

export function DoctorPage() {
  const configuration = useConfiguration();
  const [repository, setRepository] = useState("");
  const [submitted, setSubmitted] = useState<string | null>(null);
  const doctor = useDoctor(submitted, submitted !== null);

  const groups = new Map<
    string,
    typeof doctor.data extends undefined ? never : NonNullable<typeof doctor.data>["checks"]
  >();
  for (const check of doctor.data?.checks ?? []) {
    const existing = groups.get(check.category) ?? [];
    existing.push(check);
    groups.set(check.category, existing);
  }

  return (
    <div className="mx-auto flex max-w-5xl flex-col gap-4 p-6">
      <div>
        <h1 className="text-xl font-semibold text-[var(--text-primary)]">Doctor</h1>
        <p className="mt-1 text-sm text-[var(--text-muted)]">
          Inspect Git, the agent driver, every configured command, the quality providers, worktree
          permissions and disk space before a run starts.
        </p>
      </div>

      <Panel title="Target repository">
        <div className="flex flex-wrap items-end gap-3">
          <Field label="Repository path" className="min-w-72 flex-1">
            {(identifier, describedBy) => (
              <input
                id={identifier}
                aria-describedby={describedBy}
                className={inputClasses}
                value={repository}
                list="doctor-recent-repositories"
                placeholder="/home/you/projects/example"
                onChange={(event) => {
                  setRepository(event.target.value);
                }}
              />
            )}
          </Field>
          <datalist id="doctor-recent-repositories">
            {(configuration.data?.recent_repositories ?? []).map((path) => (
              <option key={path} value={path} />
            ))}
          </datalist>
          <Button
            tone="primary"
            icon={<Stethoscope aria-hidden className="size-4" />}
            onClick={() => {
              setSubmitted(repository.trim().length > 0 ? repository.trim() : null);
            }}
          >
            Run the diagnosis
          </Button>
        </div>
        {configuration.data !== undefined && (
          <p className="mt-3 text-xs text-[var(--text-muted)]">
            Application data root:{" "}
            <span className="font-mono">{configuration.data.heikas_home}</span>
          </p>
        )}
      </Panel>

      {submitted === null && (
        <Panel title="No diagnosis yet">
          <EmptyState
            title="Choose a repository"
            description="Supply the path to a local Git working tree and run the diagnosis to see every check."
          />
        </Panel>
      )}

      {submitted !== null && doctor.isPending && <LoadingState label="Running the diagnosis" />}

      {doctor.isError && (
        <ErrorState
          title="The diagnosis could not run"
          message={doctor.error.message}
          onRetry={() => {
            void doctor.refetch();
          }}
        />
      )}

      {doctor.data !== undefined && (
        <>
          <div className="flex flex-wrap items-center gap-2">
            <Badge tone={doctor.data.ready ? "success" : "failure"}>
              {doctor.data.ready ? "Ready to run" : "Blocked"}
            </Badge>
            <Badge tone={doctor.data.free_path_available ? "success" : "warning"}>
              {doctor.data.free_path_available
                ? "A free local agent path is available"
                : "No free local agent path detected"}
            </Badge>
          </div>

          {Array.from(groups.entries()).map(([category, checks]) => (
            <Panel key={category} title={category} bodyClassName="p-0">
              <ul>
                {checks.map((check) => (
                  <li
                    key={check.id}
                    className="flex flex-wrap items-start justify-between gap-3 border-b border-[var(--border-subtle)] px-4 py-3 last:border-b-0"
                  >
                    <div className="min-w-0">
                      <p className="text-sm font-medium text-[var(--text-primary)]">
                        {check.title}
                      </p>
                      <p className="mt-0.5 text-xs text-[var(--text-secondary)]">{check.detail}</p>
                      {check.remedy !== null && (
                        <p className="mt-1 text-xs text-[var(--accent-secondary)]">
                          {check.remedy}
                        </p>
                      )}
                    </div>
                    <Badge tone={OUTCOME_TONE[check.outcome]}>{check.outcome}</Badge>
                  </li>
                ))}
              </ul>
            </Panel>
          ))}

          {doctor.data.adapters.length > 0 && (
            <Panel title="Adapter matrix" bodyClassName="p-0">
              <div className="scrollbar-slim overflow-x-auto">
                <table className="w-full min-w-[720px] text-xs">
                  <caption className="sr-only">Available agent and quality adapters</caption>
                  <thead>
                    <tr className="border-b border-[var(--border-subtle)] text-left text-[var(--text-muted)]">
                      <th scope="col" className="px-4 py-2 font-medium">
                        Adapter
                      </th>
                      <th scope="col" className="px-4 py-2 font-medium">
                        Kind
                      </th>
                      <th scope="col" className="px-4 py-2 font-medium">
                        Available
                      </th>
                      <th scope="col" className="px-4 py-2 font-medium">
                        Paid account
                      </th>
                      <th scope="col" className="px-4 py-2 font-medium">
                        Isolation
                      </th>
                      <th scope="col" className="px-4 py-2 font-medium">
                        Detail
                      </th>
                    </tr>
                  </thead>
                  <tbody>
                    {doctor.data.adapters.map((adapter) => (
                      <tr
                        key={`${adapter.kind}-${adapter.name}`}
                        className="border-b border-[var(--border-subtle)] last:border-b-0"
                      >
                        <td className="px-4 py-2 text-[var(--text-primary)]">{adapter.name}</td>
                        <td className="px-4 py-2 text-[var(--text-secondary)]">{adapter.kind}</td>
                        <td className="px-4 py-2">
                          <Badge tone={adapter.available ? "success" : "neutral"}>
                            {adapter.available ? "yes" : "no"}
                          </Badge>
                        </td>
                        <td className="px-4 py-2">
                          <Badge tone={adapter.requires_paid_account ? "warning" : "success"}>
                            {adapter.requires_paid_account ? "required" : "not required"}
                          </Badge>
                        </td>
                        <td className="px-4 py-2 text-[var(--text-secondary)]">
                          {adapter.isolation ?? "not applicable"}
                        </td>
                        <td className="px-4 py-2 text-[var(--text-muted)]">{adapter.detail}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </Panel>
          )}
        </>
      )}
    </div>
  );
}
