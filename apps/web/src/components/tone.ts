export type BadgeTone = "neutral" | "success" | "warning" | "failure" | "info" | "accent";

export function runStatusTone(status: string): BadgeTone {
  switch (status) {
    case "succeeded":
      return "success";
    case "failed":
    case "exhausted":
      return "failure";
    case "cancelled":
    case "recovery_required":
      return "warning";
    case "awaiting_plan_approval":
    case "awaiting_commit_approval":
      return "info";
    case "created":
      return "neutral";
    default:
      return "accent";
  }
}

export function candidateStatusTone(status: string): BadgeTone {
  switch (status) {
    case "eligible":
      return "success";
    case "ineligible":
    case "cancelled":
      return "failure";
    case "interrupted":
      return "warning";
    case "pending":
      return "neutral";
    default:
      return "accent";
  }
}

export function checkOutcomeTone(outcome: string): BadgeTone {
  switch (outcome) {
    case "passed":
      return "success";
    case "warning":
      return "warning";
    case "failed":
      return "failure";
    default:
      return "neutral";
  }
}
