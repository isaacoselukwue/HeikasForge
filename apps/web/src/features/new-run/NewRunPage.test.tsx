import { describe, expect, it, vi } from "vitest";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { NewRunPage } from "./NewRunPage";
import { describeViolations, findAccessibilityViolations, renderComponent } from "@/testing/render";

const navigate = vi.fn();
const createRun = vi.fn();

vi.mock("@tanstack/react-router", () => ({
  useNavigate: () => navigate,
  Link: ({ children }: { children: React.ReactNode }) => <a href="/">{children}</a>,
}));

vi.mock("@/app/sessionContext", () => ({
  useSession: () => ({ demonstrationMode: false, initialRunId: null }),
}));

vi.mock("@/api/queries", () => ({
  useConfiguration: () => ({
    isPending: false,
    isError: false,
    data: {
      heikas_home: "/home/operator/.local/share/heikas-forge",
      user_configuration_path: "/home/operator/.local/share/heikas-forge/config/forge.toml",
      demonstration_mode: false,
      default_candidate_count: 3,
      maximum_candidate_count: 8,
      agent_drivers: [
        {
          id: "local",
          label: "Built-in local tool agent",
          requires_paid_account: false,
          demonstration_only: false,
        },
        {
          id: "fake",
          label: "Deterministic demonstration agent",
          requires_paid_account: false,
          demonstration_only: true,
        },
      ],
      quality_profiles: ["standard", "strict"],
      commit_policies: ["manual", "automatic", "none"],
      recent_repositories: ["/home/operator/projects/example"],
    },
  }),
  useCreateRun: () => ({ mutate: createRun, isPending: false, isError: false, error: null }),
  useDoctor: () => ({ isPending: false, isError: false, data: undefined, refetch: vi.fn() }),
}));

describe("NewRunPage", () => {
  it("prevents submission until the repository and task are supplied", () => {
    renderComponent(<NewRunPage />);
    const submit = screen.getByRole("button", { name: /Create run and plan/ });
    expect(submit).toBeDisabled();
    expect(screen.getByText("Supply the path to a local Git repository.")).toBeInTheDocument();
    expect(
      screen.getByText("Describe the task in at least a sentence so the plan can be specific."),
    ).toBeInTheDocument();
    expect(
      screen.getByText("2 fields still need attention before the run can start."),
    ).toBeInTheDocument();
  });

  it("explains the remedy next to the offending field and enables submission once valid", async () => {
    renderComponent(<NewRunPage />);
    await userEvent.type(
      screen.getByLabelText("Repository path"),
      "/home/operator/projects/example",
    );
    await userEvent.type(
      screen.getByLabelText("Task"),
      "Round invoice currency half away from zero so ties never fall towards zero.",
    );
    const submit = screen.getByRole("button", { name: /Create run and plan/ });
    expect(submit).toBeEnabled();
    await userEvent.click(submit);
    expect(createRun).toHaveBeenCalledOnce();
    const payload = createRun.mock.calls[0]?.[0] as {
      repository_path: string;
      candidate_count: number;
    };
    expect(payload.repository_path).toBe("/home/operator/projects/example");
    expect(payload.candidate_count).toBe(3);
  });

  it("rejects a parallel candidate count above the candidate count", async () => {
    renderComponent(<NewRunPage />);
    const parallel = screen.getByLabelText("Maximum parallel candidates");
    await userEvent.clear(parallel);
    await userEvent.type(parallel, "8");
    expect(
      screen.getByText("Parallel candidates cannot exceed the candidate count."),
    ).toBeInTheDocument();
  });

  it("states that planning is read only before any candidate is created", () => {
    renderComponent(<NewRunPage />);
    expect(
      screen.getByText(/No candidate worktree is created until you approve the plan/),
    ).toBeInTheDocument();
  });

  it("has no accessibility violations", async () => {
    const { container } = renderComponent(<NewRunPage />);
    const violations = await findAccessibilityViolations(container);
    expect(violations, describeViolations(violations)).toHaveLength(0);
  });
});
