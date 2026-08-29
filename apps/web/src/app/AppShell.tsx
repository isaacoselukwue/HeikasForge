import { useEffect, useState } from "react";
import type { ReactNode } from "react";
import { Link, useNavigate, useRouterState } from "@tanstack/react-router";
import {
  Activity,
  BookOpen,
  ChevronLeft,
  ChevronRight,
  Command,
  LayoutDashboard,
  Moon,
  PlusCircle,
  Settings,
  Stethoscope,
  Sun,
} from "lucide-react";

import { Badge } from "@/components/Badge";
import { runStatusTone } from "@/components/tone";
import { Button } from "@/components/Button";
import { CommandPalette } from "@/components/CommandPalette";
import type { PaletteCommand } from "@/components/CommandPalette";
import { Tooltip, TooltipProvider } from "@/components/Tooltip";
import { cx } from "@/components/classNames";
import { useHealth, useRuns } from "@/api/queries";
import { useSession } from "./sessionContext";
import { useTheme } from "./themeContext";
import { shortRunId } from "./format";

const NAVIGATION = [
  { to: "/", label: "Runs", icon: LayoutDashboard },
  { to: "/new", label: "New run", icon: PlusCircle },
  { to: "/doctor", label: "Doctor", icon: Stethoscope },
  { to: "/settings", label: "Settings", icon: Settings },
  { to: "/documentation", label: "Documentation", icon: BookOpen },
] as const;

export function AppShell({ children }: { children: ReactNode }) {
  const [collapsed, setCollapsed] = useState(false);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const navigate = useNavigate();
  const health = useHealth();
  const runs = useRuns();
  const session = useSession();
  const { preference, resolved, setPreference } = useTheme();
  const routerState = useRouterState();
  const activePath = routerState.location.pathname;

  useEffect(() => {
    const listener = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        setPaletteOpen((open) => !open);
      }
    };
    window.addEventListener("keydown", listener);
    return () => {
      window.removeEventListener("keydown", listener);
    };
  }, []);

  const activeRun = (runs.data ?? []).find(
    (run) => !["succeeded", "failed", "cancelled", "exhausted"].includes(run.status),
  );

  const commands: PaletteCommand[] = [
    {
      id: "goto-runs",
      label: "Open the run dashboard",
      group: "Navigation",
      perform: () => {
        void navigate({ to: "/" });
      },
    },
    {
      id: "goto-new-run",
      label: "Start a new run",
      group: "Navigation",
      perform: () => {
        void navigate({ to: "/new" });
      },
    },
    {
      id: "goto-doctor",
      label: "Run the environment doctor",
      group: "Navigation",
      perform: () => {
        void navigate({ to: "/doctor" });
      },
    },
    {
      id: "goto-settings",
      label: "Open settings",
      group: "Navigation",
      perform: () => {
        void navigate({ to: "/settings" });
      },
    },
    {
      id: "toggle-theme",
      label: resolved === "dark" ? "Switch to the light theme" : "Switch to the dark theme",
      group: "Appearance",
      perform: () => {
        setPreference(resolved === "dark" ? "light" : "dark");
      },
    },
    ...(runs.data ?? []).slice(0, 8).map((run) => ({
      id: `open-run-${run.run_id}`,
      label: `Open run ${shortRunId(run.run_id)}: ${run.task_title}`,
      group: "Runs",
      hint: run.status_label,
      perform: () => {
        void navigate({ to: "/runs/$runId", params: { runId: run.run_id } });
      },
    })),
  ];

  return (
    <TooltipProvider>
      <a href="#main-content" className="skip-link">
        Skip to the main content
      </a>
      <div className="flex h-screen w-full overflow-hidden bg-[var(--surface-canvas)]">
        <nav
          aria-label="Primary"
          className={cx(
            "flex shrink-0 flex-col border-r border-[var(--border-subtle)] bg-[var(--surface-raised)] transition-[width] duration-[var(--duration-medium)]",
            collapsed ? "w-16" : "w-60",
          )}
        >
          <div className="flex h-14 items-center gap-2 border-b border-[var(--border-subtle)] px-3">
            <span
              aria-hidden
              className="grid size-8 shrink-0 place-items-center rounded-[var(--radius-medium)] bg-[color-mix(in_srgb,var(--accent-primary)_22%,transparent)] text-[var(--accent-primary)]"
            >
              <Activity className="size-4" />
            </span>
            {!collapsed && (
              <span className="truncate text-sm font-semibold text-[var(--text-primary)]">
                Heikas Forge
              </span>
            )}
          </div>
          <ul className="flex-1 space-y-1 p-2">
            {NAVIGATION.map((item) => {
              const Icon = item.icon;
              const active = item.to === "/" ? activePath === "/" : activePath.startsWith(item.to);
              return (
                <li key={item.to}>
                  <Link
                    to={item.to}
                    aria-current={active ? "page" : undefined}
                    className={cx(
                      "flex items-center gap-3 rounded-[var(--radius-medium)] px-3 py-2 text-sm transition-colors",
                      active
                        ? "bg-[color-mix(in_srgb,var(--accent-primary)_16%,transparent)] text-[var(--accent-primary)]"
                        : "text-[var(--text-secondary)] hover:bg-[var(--surface-overlay)] hover:text-[var(--text-primary)]",
                    )}
                  >
                    <Icon aria-hidden className="size-4 shrink-0" />
                    <span className={cx(collapsed && "sr-only")}>{item.label}</span>
                  </Link>
                </li>
              );
            })}
          </ul>
          <div className="border-t border-[var(--border-subtle)] p-2">
            <Button
              tone="ghost"
              size="small"
              className="w-full justify-start"
              onClick={() => {
                setCollapsed((value) => !value);
              }}
              aria-expanded={!collapsed}
              icon={
                collapsed ? (
                  <ChevronRight aria-hidden className="size-4" />
                ) : (
                  <ChevronLeft aria-hidden className="size-4" />
                )
              }
            >
              <span className={cx(collapsed && "sr-only")}>Collapse navigation</span>
            </Button>
          </div>
        </nav>

        <div className="flex min-w-0 flex-1 flex-col">
          <header className="flex h-14 shrink-0 items-center justify-between gap-4 border-b border-[var(--border-subtle)] bg-[var(--surface-raised)] px-4">
            <div className="flex min-w-0 items-center gap-3">
              {activeRun !== undefined ? (
                <Link
                  to="/runs/$runId"
                  params={{ runId: activeRun.run_id }}
                  className="flex min-w-0 items-center gap-2 text-sm text-[var(--text-secondary)] hover:text-[var(--text-primary)]"
                >
                  <Badge tone={runStatusTone(activeRun.status)}>{activeRun.status_label}</Badge>
                  <span className="truncate">{activeRun.task_title}</span>
                  <span className="hidden truncate text-xs text-[var(--text-muted)] md:inline">
                    {activeRun.repository_path}
                  </span>
                </Link>
              ) : (
                <span className="text-sm text-[var(--text-muted)]">No active run</span>
              )}
            </div>
            <div className="flex items-center gap-2">
              {session.demonstrationMode && (
                <Badge tone="warning" title="Demonstration mode replays a recorded fixture">
                  Demonstration mode
                </Badge>
              )}
              {health.data !== undefined && (
                <Badge tone="neutral" title="Local orchestrator version">
                  v{health.data.version}
                </Badge>
              )}
              <Tooltip content="Open the command palette with Control or Command and K">
                <Button
                  tone="ghost"
                  size="small"
                  onClick={() => {
                    setPaletteOpen(true);
                  }}
                  icon={<Command aria-hidden className="size-4" />}
                >
                  Commands
                </Button>
              </Tooltip>
              <Tooltip content={resolved === "dark" ? "Use the light theme" : "Use the dark theme"}>
                <Button
                  tone="ghost"
                  size="small"
                  aria-label={
                    resolved === "dark" ? "Switch to the light theme" : "Switch to the dark theme"
                  }
                  onClick={() => {
                    setPreference(preference === "dark" ? "light" : "dark");
                  }}
                  icon={
                    resolved === "dark" ? (
                      <Sun aria-hidden className="size-4" />
                    ) : (
                      <Moon aria-hidden className="size-4" />
                    )
                  }
                />
              </Tooltip>
            </div>
          </header>

          <main id="main-content" className="min-h-0 flex-1 overflow-auto scrollbar-slim">
            {children}
          </main>
        </div>
      </div>
      <CommandPalette commands={commands} open={paletteOpen} onOpenChange={setPaletteOpen} />
    </TooltipProvider>
  );
}
