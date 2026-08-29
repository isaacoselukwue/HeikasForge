import { describe, expect, it } from "vitest";
import { screen } from "@testing-library/react";

import { MarkdownView } from "./MarkdownView";
import { describeViolations, findAccessibilityViolations, renderComponent } from "@/testing/render";

describe("MarkdownView", () => {
  it("renders headings, lists and fenced code without interpreting markup", () => {
    const markdown = [
      "# Implementation plan",
      "",
      "## Files expected to change",
      "",
      "- `src/invoice.py`",
      "- src/rounding.py",
      "",
      "```",
      "not a heading",
      "```",
      "",
      "A paragraph with **strong** and `code` text.",
    ].join("\n");
    renderComponent(<MarkdownView markdown={markdown} />);
    expect(screen.getByRole("heading", { name: "Implementation plan" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Files expected to change" })).toBeInTheDocument();
    expect(screen.getByText("src/invoice.py")).toBeInTheDocument();
    expect(screen.getByText("not a heading")).toBeInTheDocument();
    expect(screen.getByText("strong")).toBeInTheDocument();
  });

  it("never injects untrusted markup as html", () => {
    const { container } = renderComponent(
      <MarkdownView markdown={"## Risks\n\n<img src=x onerror=alert(1)>\n"} />,
    );
    expect(container.querySelector("img")).toBeNull();
    expect(container.textContent).toContain("<img src=x onerror=alert(1)>");
  });

  it("has no accessibility violations", async () => {
    const { container } = renderComponent(
      <MarkdownView markdown={"## Assumptions\n\nThe precision stays at two places.\n"} />,
    );
    const violations = await findAccessibilityViolations(container);
    expect(violations, describeViolations(violations)).toHaveLength(0);
  });
});
