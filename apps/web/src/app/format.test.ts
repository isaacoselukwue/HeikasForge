import { describe, expect, it } from "vitest";

import {
  formatBytes,
  formatDuration,
  formatRelative,
  humaniseIdentifier,
  shortRunId,
} from "./format";

describe("formatting", () => {
  it("formats durations across every magnitude", () => {
    expect(formatDuration(250)).toBe("250ms");
    expect(formatDuration(4_500)).toBe("4s");
    expect(formatDuration(95_000)).toBe("1m 35s");
    expect(formatDuration(3_930_000)).toBe("1h 5m");
  });

  it("formats byte counts", () => {
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(2_048)).toBe("2.0 KB");
    expect(formatBytes(5_242_880)).toBe("5.0 MB");
  });

  it("shortens a run identifier to a stable prefix", () => {
    expect(shortRunId("01a04e12-aa14-7d70-9a0e-d6844363b1af")).toBe("01a04e12aa14");
  });

  it("humanises an identifier for display", () => {
    expect(humaniseIdentifier("awaiting_plan_approval")).toBe("Awaiting plan approval");
    expect(humaniseIdentifier("minimal-patch")).toBe("Minimal patch");
  });

  it("describes a relative time", () => {
    const now = Date.parse("2026-08-29T12:00:00Z");
    expect(formatRelative("2026-08-29T11:58:00Z", now)).toBe("2m 0s ago");
  });
});
