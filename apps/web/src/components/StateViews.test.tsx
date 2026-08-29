import { describe, expect, it, vi } from "vitest";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { EmptyState, ErrorState, LoadingState } from "./StateViews";
import { describeViolations, findAccessibilityViolations, renderComponent } from "@/testing/render";

describe("state views", () => {
  it("announces a loading state politely", () => {
    renderComponent(<LoadingState label="Loading runs" />);
    const status = screen.getByRole("status");
    expect(status).toHaveAttribute("aria-live", "polite");
    expect(status).toHaveTextContent("Loading runs");
  });

  it("explains an empty state with a next action", () => {
    renderComponent(
      <EmptyState
        title="Nothing yet"
        description="Create a run to begin."
        action={<button type="button">Start</button>}
      />,
    );
    expect(screen.getByRole("heading", { name: "Nothing yet" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Start" })).toBeInTheDocument();
  });

  it("never shows a generic failure message and always offers a next action", async () => {
    const retry = vi.fn();
    renderComponent(
      <ErrorState
        title="The run could not be loaded"
        message="The orchestrator refused the request."
        remedy="Confirm that the orchestrator is still running."
        lastDurableEvent="Plan version 1 written"
        sourceChangesPossible={false}
        onRetry={retry}
      />,
    );
    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent("The run could not be loaded");
    expect(alert).not.toHaveTextContent("Something went wrong");
    expect(screen.getByText("Confirm that the orchestrator is still running.")).toBeInTheDocument();
    expect(screen.getByText("Plan version 1 written")).toBeInTheDocument();
    expect(screen.getByText("No candidate source was changed.")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect(retry).toHaveBeenCalledOnce();
  });

  it("offers a redacted diagnostic bundle", () => {
    renderComponent(<ErrorState title="Failure" message="A failure occurred." />);
    expect(screen.getByRole("button", { name: /Copy redacted diagnostic/ })).toBeInTheDocument();
  });

  it("has no accessibility violations", async () => {
    const { container } = renderComponent(
      <ErrorState title="Failure" message="A failure occurred." remedy="Try again." />,
    );
    const violations = await findAccessibilityViolations(container);
    expect(violations, describeViolations(violations)).toHaveLength(0);
  });
});
