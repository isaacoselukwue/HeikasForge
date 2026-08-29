import { useGraphDefinition } from "@/api/queries";
import { Badge } from "@/components/Badge";
import { Panel } from "@/components/Panel";
import { LoadingState } from "@/components/StateViews";

export function DocumentationPage() {
  const graph = useGraphDefinition();

  return (
    <div className="mx-auto flex max-w-4xl flex-col gap-4 p-6">
      <div>
        <h1 className="text-xl font-semibold text-[var(--text-primary)]">Documentation</h1>
        <p className="mt-1 text-sm text-[var(--text-muted)]">
          How a run moves through the graph, what each gate decides and where the evidence is
          written.
        </p>
      </div>

      <Panel title="Run lifecycle">
        <ol className="flex flex-col gap-3 text-sm text-[var(--text-secondary)]">
          {[
            [
              "Prepare",
              "Validate Git, resolve the baseline commit, snapshot the effective configuration and check every configured command.",
            ],
            [
              "Plan",
              "A read-only agent inspects the repository and writes a plan version. No file is changed.",
            ],
            [
              "Plan approval",
              "The run pauses indefinitely. Approval records the exact plan hash, and any later edit invalidates it.",
            ],
            [
              "Fan out",
              "Candidate worktrees are created from the same immutable baseline, each with its own strategy and repair budget.",
            ],
            [
              "Implement, test, review, repair",
              "Each candidate runs its own subgraph. Failing gates route into a bounded repair loop.",
            ],
            [
              "Join",
              "Ineligible candidates are excluded with reasons. Eligible candidates are ranked with a deterministic tuple.",
            ],
            [
              "Integrate and final gates",
              "The winning patch is applied to a clean integration worktree and every required gate runs again.",
            ],
            [
              "Commit",
              "A dedicated branch is created and committed once the configured approval policy is satisfied. Nothing is pushed.",
            ],
          ].map(([title, description]) => (
            <li key={title} className="flex gap-3">
              <span
                aria-hidden
                className="mt-1.5 size-1.5 shrink-0 rounded-full bg-[var(--accent-primary)]"
              />
              <span>
                <strong className="font-semibold text-[var(--text-primary)]">{title}.</strong>{" "}
                {description}
              </span>
            </li>
          ))}
        </ol>
      </Panel>

      <Panel title="Registered nodes">
        {graph.isPending ? (
          <LoadingState label="Loading the graph definition" />
        ) : graph.data === undefined ? (
          <p className="text-sm text-[var(--text-muted)]">The graph definition is unavailable.</p>
        ) : (
          <ul className="grid gap-2 sm:grid-cols-2">
            {graph.data.nodes.map((node) => (
              <li
                key={node.id}
                className="flex items-center justify-between gap-2 rounded-[var(--radius-medium)] border border-[var(--border-subtle)] px-3 py-2 text-xs"
              >
                <span className="min-w-0">
                  <span className="block text-[var(--text-primary)]">{node.label}</span>
                  <span className="block font-mono text-[var(--text-muted)]">{node.id}</span>
                </span>
                <span className="flex shrink-0 items-center gap-1">
                  <Badge tone={node.scope === "candidate" ? "accent" : "neutral"}>
                    {node.scope}
                  </Badge>
                  {node.read_only && <Badge tone="info">read only</Badge>}
                </span>
              </li>
            ))}
          </ul>
        )}
      </Panel>

      <Panel title="Where evidence is written">
        <ul className="flex flex-col gap-2 text-xs text-[var(--text-secondary)]">
          {[
            [
              "events.jsonl",
              "The authoritative hash-chained history. Every projection is rebuilt from it.",
            ],
            [
              "state.json",
              "A rebuildable projection that records the applied event sequence and hash.",
            ],
            ["plan/", "Every plan version and the durable approval record."],
            [
              "nodes/",
              "One immutable directory per node attempt with input, invocation, result and streams.",
            ],
            ["candidates/", "Per candidate diff, reports and score."],
            ["integration/", "The ranking, the selected candidate and the integration diff."],
            ["logs/run.jsonl", "Redacted structured log records."],
            ["exports/", "Redacted evidence archives."],
          ].map(([path, description]) => (
            <li key={path} className="flex flex-wrap gap-2">
              <code className="rounded bg-[var(--surface-sunken)] px-1.5 py-0.5 text-[var(--accent-secondary)]">
                {path}
              </code>
              <span>{description}</span>
            </li>
          ))}
        </ul>
      </Panel>

      <Panel title="Safety boundaries">
        <ul className="list-disc space-y-1 pl-5 text-xs text-[var(--text-secondary)]">
          <li>Agents receive no general shell. Only named configured commands are available.</li>
          <li>
            Every path is confined to the assigned worktree and checked against the protected globs.
          </li>
          <li>Agents may not commit, push, fetch, merge, rebase or change Git configuration.</li>
          <li>Task text is never interpolated into a shell command.</li>
          <li>
            Existing tests, coverage thresholds and quality configuration cannot be weakened
            silently.
          </li>
          <li>The default branch is never modified. A dedicated branch receives the commit.</li>
        </ul>
      </Panel>
    </div>
  );
}
