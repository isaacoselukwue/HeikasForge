import { describe, expect, it } from "vitest";
import { screen } from "@testing-library/react";

import { DiffViewer } from "./DiffViewer";
import { describeViolations, findAccessibilityViolations, renderComponent } from "@/testing/render";

const PATCH = `diff --git a/src/invoice.py b/src/invoice.py
index a4991fa..237e9a3 100644
--- a/src/invoice.py
+++ b/src/invoice.py
@@ -1,4 +1,4 @@
-from decimal import Decimal, ROUND_HALF_EVEN
+from decimal import Decimal, ROUND_HALF_UP
 
 
 LINE_ITEM_PRECISION = Decimal("0.01")`;

describe("DiffViewer", () => {
  it("renders every patch line with a line number", () => {
    renderComponent(<DiffViewer patch={PATCH} label="Candidate diff" />);
    expect(screen.getByRole("group", { name: "Candidate diff" })).toBeInTheDocument();
    expect(screen.getByText(/ROUND_HALF_UP/)).toBeInTheDocument();
    expect(screen.getByText(/ROUND_HALF_EVEN/)).toBeInTheDocument();
  });

  it("explains an empty diff instead of rendering nothing", () => {
    renderComponent(<DiffViewer patch="" label="Candidate diff" />);
    expect(screen.getByText("No changes recorded")).toBeInTheDocument();
    expect(
      screen.getByText(/produced no difference against the baseline commit/),
    ).toBeInTheDocument();
  });

  it("reports when the rendered patch was truncated", () => {
    const long = Array.from({ length: 40 }, (_, index) => `+line ${String(index)}`).join("\n");
    renderComponent(<DiffViewer patch={long} label="Candidate diff" maximumLines={10} />);
    expect(screen.getByText(/The first 10 lines are shown/)).toBeInTheDocument();
  });

  it("has no accessibility violations", async () => {
    const { container } = renderComponent(<DiffViewer patch={PATCH} label="Candidate diff" />);
    const violations = await findAccessibilityViolations(container);
    expect(violations, describeViolations(violations)).toHaveLength(0);
  });
});
