import { describe, expect, it } from "vitest";

import { candidateStatusTone, checkOutcomeTone, runStatusTone } from "./tone";

describe("status tones", () => {
  it("maps every run status to a stable tone", () => {
    expect(runStatusTone("succeeded")).toBe("success");
    expect(runStatusTone("failed")).toBe("failure");
    expect(runStatusTone("exhausted")).toBe("failure");
    expect(runStatusTone("cancelled")).toBe("warning");
    expect(runStatusTone("recovery_required")).toBe("warning");
    expect(runStatusTone("awaiting_plan_approval")).toBe("info");
    expect(runStatusTone("awaiting_commit_approval")).toBe("info");
    expect(runStatusTone("running_candidates")).toBe("accent");
  });

  it("maps every candidate status to a stable tone", () => {
    expect(candidateStatusTone("eligible")).toBe("success");
    expect(candidateStatusTone("ineligible")).toBe("failure");
    expect(candidateStatusTone("cancelled")).toBe("failure");
    expect(candidateStatusTone("interrupted")).toBe("warning");
    expect(candidateStatusTone("pending")).toBe("neutral");
    expect(candidateStatusTone("testing")).toBe("accent");
  });

  it("maps every doctor outcome to a stable tone", () => {
    expect(checkOutcomeTone("passed")).toBe("success");
    expect(checkOutcomeTone("warning")).toBe("warning");
    expect(checkOutcomeTone("failed")).toBe("failure");
    expect(checkOutcomeTone("skipped")).toBe("neutral");
  });
});
