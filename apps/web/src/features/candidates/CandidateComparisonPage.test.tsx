import { describe, expect, it, vi } from "vitest";
import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { CandidateComparisonPage } from "./CandidateComparisonPage";
import { describeViolations, findAccessibilityViolations, renderComponent } from "@/testing/render";
import type { CandidateView, RunDetail } from "@/generated/api-types";

const approveCommit = vi.fn();

function candidate(
  ordinal: number,
  status: string,
  rank: number | null,
  changedLines: number,
  exclusions: string[],
): CandidateView {
  return {
    candidate_id: `c0${String(ordinal)}-demo`,
    ordinal,
    strategy: "minimal_patch",
    strategy_label:
      ordinal === 1 ? "Minimal patch" : ordinal === 2 ? "Test led" : "Architecture aware",
    status: status as CandidateView["status"],
    status_label: status === "eligible" ? "Eligible" : "Ineligible",
    branch: `heikas/work/c0${String(ordinal)}`,
    repairs_used: ordinal === 2 ? 1 : 0,
    repair_budget: 2,
    changed_files: 1,
    changed_lines: changedLines,
    gate_duration: 1_500 * ordinal,
    score: null,
    score_components: [],
    exclusion_reasons: [],
    exclusion_summaries: exclusions,
    rank,
    is_winner: rank === 1,
    promotable: rank !== null,
    tests_passed: status === "eligible",
    review_passed: status === "eligible",
    line_coverage_percent: 100,
  };
}

const CANDIDATES = [
  candidate(1, "eligible", 1, 4, []),
  candidate(2, "eligible", 2, 19, []),
  candidate(3, "ineligible", null, 24, [
    "Blocker policy finding `existing-test-removed`: an existing test was removed",
  ]),
];

const DETAIL = {
  summary: { status: "awaiting_commit_approval" },
  candidates: CANDIDATES,
  ranking_rationale: [
    "2 of 3 candidates satisfied every required gate.",
    "Candidate c01-demo ranked first on the deterministic tuple, decided by Changed lines (4 against 19).",
  ],
  projection: {
    integration: {
      final_tests_passed: true,
      final_review_passed: true,
      applied_candidate: "c01-demo",
    },
    commit: null,
  },
} as unknown as RunDetail;

vi.mock("@tanstack/react-router", () => ({
  Link: ({ children }: { children: React.ReactNode }) => <a href="/">{children}</a>,
}));

vi.mock("@/api/queries", () => ({
  useRunDetail: () => ({ isPending: false, isError: false, data: DETAIL, refetch: vi.fn() }),
  useApproveCommit: () => ({ mutate: approveCommit, isPending: false }),
  useCandidateDiff: () => ({ data: undefined, isPending: true, isError: false }),
}));

function rowOrder(): string[] {
  const rows = screen.getAllByRole("row").slice(1);
  return rows.map((row) => within(row).getAllByRole("cell")[1]?.textContent ?? "");
}

describe("CandidateComparisonPage", () => {
  it("shows the winner banner with the deterministic rationale", () => {
    renderComponent(<CandidateComparisonPage runId="demo" />);
    expect(screen.getByRole("heading", { name: /Winner c01-demo/ })).toBeInTheDocument();
    expect(screen.getByText(/decided by Changed lines \(4 against 19\)/)).toBeInTheDocument();
    expect(screen.getByText(/tests passed/)).toBeInTheDocument();
  });

  it("orders by rank by default and re-sorts when a column heading is activated", async () => {
    renderComponent(<CandidateComparisonPage runId="demo" />);
    expect(rowOrder()).toEqual(["c01-demo", "c02-demo", "c03-demo"]);

    await userEvent.click(screen.getByRole("button", { name: /Changed lines/ }));
    expect(rowOrder()).toEqual(["c03-demo", "c02-demo", "c01-demo"]);

    await userEvent.click(screen.getByRole("button", { name: /Changed lines/ }));
    expect(rowOrder()).toEqual(["c01-demo", "c02-demo", "c03-demo"]);
  });

  it("declares the sort state for assistive technology", async () => {
    renderComponent(<CandidateComparisonPage runId="demo" />);
    expect(screen.getAllByRole("columnheader")[0]).toHaveAttribute("aria-sort", "ascending");
    expect(screen.getAllByRole("columnheader")[1]).toHaveAttribute("aria-sort", "none");
    await userEvent.click(screen.getByRole("button", { name: /Candidate/ }));
    expect(screen.getAllByRole("columnheader")[0]).toHaveAttribute("aria-sort", "none");
    expect(screen.getAllByRole("columnheader")[1]).toHaveAttribute("aria-sort", "ascending");
  });

  it("states why an ineligible candidate was excluded", () => {
    renderComponent(<CandidateComparisonPage runId="demo" />);
    expect(screen.getByRole("heading", { name: "Exclusion reasons" })).toBeInTheDocument();
    expect(screen.getByText(/Blocker policy finding `existing-test-removed`/)).toBeInTheDocument();
  });

  it("offers commit approval only while the run is awaiting it", async () => {
    renderComponent(<CandidateComparisonPage runId="demo" />);
    await userEvent.click(screen.getByRole("button", { name: /Approve the commit/ }));
    expect(approveCommit).toHaveBeenCalledOnce();
  });

  it("has no accessibility violations", async () => {
    const { container } = renderComponent(<CandidateComparisonPage runId="demo" />);
    const violations = await findAccessibilityViolations(container);
    expect(violations, describeViolations(violations)).toHaveLength(0);
  });
});
