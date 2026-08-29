import { useConfiguration } from "@/api/queries";
import { Badge } from "@/components/Badge";
import { Panel } from "@/components/Panel";
import { SelectField } from "@/components/SelectField";
import { ErrorState, LoadingState } from "@/components/StateViews";
import { useTheme } from "@/app/themeContext";
import type { ThemePreference } from "@/app/themeContext";

export function SettingsPage() {
  const configuration = useConfiguration();
  const { preference, setPreference } = useTheme();

  if (configuration.isPending) {
    return <LoadingState label="Loading settings" />;
  }

  if (configuration.isError) {
    return (
      <div className="p-6">
        <ErrorState
          title="Settings could not be loaded"
          message={configuration.error.message}
          onRetry={() => {
            void configuration.refetch();
          }}
        />
      </div>
    );
  }

  return (
    <div className="mx-auto flex max-w-4xl flex-col gap-4 p-6">
      <div>
        <h1 className="text-xl font-semibold text-[var(--text-primary)]">Settings</h1>
        <p className="mt-1 text-sm text-[var(--text-muted)]">
          Appearance is stored in this browser. Everything else is read from the layered
          configuration files on this machine.
        </p>
      </div>

      <Panel title="Appearance">
        <SelectField
          label="Theme"
          value={preference}
          onValueChange={(value) => {
            setPreference(value as ThemePreference);
          }}
          options={[
            { value: "dark", label: "Dark", description: "The default control room palette" },
            { value: "light", label: "Light", description: "A high contrast light palette" },
            {
              value: "system",
              label: "Follow the system",
              description: "Track the operating system preference",
            },
          ]}
          hint="Reduced motion is honoured automatically from your system preference."
        />
      </Panel>

      <Panel title="Data location">
        <dl className="grid gap-3 text-xs sm:grid-cols-2">
          <div>
            <dt className="text-[var(--text-muted)]">Application data root</dt>
            <dd className="mt-0.5 break-all font-mono text-[var(--text-primary)]">
              {configuration.data.heikas_home}
            </dd>
          </div>
          <div>
            <dt className="text-[var(--text-muted)]">User configuration</dt>
            <dd className="mt-0.5 break-all font-mono text-[var(--text-primary)]">
              {configuration.data.user_configuration_path}
            </dd>
          </div>
        </dl>
        <p className="mt-3 text-xs text-[var(--text-muted)]">
          Set the HEIKAS_HOME environment variable to move the run store. Repository configuration
          is read from .heikas/forge.toml inside each target repository.
        </p>
      </Panel>

      <Panel title="Agent adapters">
        <ul className="flex flex-col gap-2">
          {configuration.data.agent_drivers.map((driver) => (
            <li
              key={driver.id}
              className="flex items-center justify-between gap-3 rounded-[var(--radius-medium)] border border-[var(--border-subtle)] px-3 py-2"
            >
              <div className="min-w-0">
                <p className="text-sm text-[var(--text-primary)]">{driver.label}</p>
                <p className="font-mono text-xs text-[var(--text-muted)]">{driver.id}</p>
              </div>
              <div className="flex shrink-0 items-center gap-2">
                {driver.demonstration_only && <Badge tone="warning">Demonstration only</Badge>}
                <Badge tone={driver.requires_paid_account ? "warning" : "success"}>
                  {driver.requires_paid_account ? "Needs its own account" : "Free local path"}
                </Badge>
              </div>
            </li>
          ))}
        </ul>
      </Panel>

      <Panel title="Quality and commit defaults">
        <dl className="grid gap-3 text-xs sm:grid-cols-2">
          <div>
            <dt className="text-[var(--text-muted)]">Quality profiles</dt>
            <dd className="mt-0.5 text-[var(--text-primary)]">
              {configuration.data.quality_profiles.join(", ")}
            </dd>
          </div>
          <div>
            <dt className="text-[var(--text-muted)]">Commit policies</dt>
            <dd className="mt-0.5 text-[var(--text-primary)]">
              {configuration.data.commit_policies.join(", ")}
            </dd>
          </div>
          <div>
            <dt className="text-[var(--text-muted)]">Default candidates</dt>
            <dd className="mt-0.5 text-[var(--text-primary)]">
              {configuration.data.default_candidate_count}
            </dd>
          </div>
          <div>
            <dt className="text-[var(--text-muted)]">Maximum candidates</dt>
            <dd className="mt-0.5 text-[var(--text-primary)]">
              {configuration.data.maximum_candidate_count}
            </dd>
          </div>
        </dl>
      </Panel>

      <Panel title="Privacy">
        <ul className="list-disc space-y-1 pl-5 text-xs text-[var(--text-secondary)]">
          <li>The interface binds to the loopback interface and makes no third-party request.</li>
          <li>No remote fonts, analytics or content delivery networks are loaded.</li>
          <li>Secrets are redacted before any log or export leaves the orchestrator.</li>
          <li>No telemetry leaves this machine.</li>
        </ul>
      </Panel>
    </div>
  );
}
