/* eslint-disable */
/**
 * This file is generated from the Rust JSON schemas by `pnpm generate:types`.
 * Do not edit it by hand and do not add a duplicate hand written transport model.
 */

export type CandidateId = string;
export type CandidateStatus =
  | "pending"
  | "preparing"
  | "implementing"
  | "testing"
  | "reviewing"
  | "repairing"
  | "eligible"
  | "ineligible"
  | "interrupted"
  | "cancelled";
export type DurationMs = number;
export type CoverageRank =
  | {
      kind: "measured";
      value: number;
    }
  | {
      kind: "missing";
    };
export type ExclusionReason =
  | {
      reason: "required_test_failed";
      command_id: string;
      detail: string;
    }
  | {
      reason: "required_test_missing";
      command_id: string;
    }
  | {
      reason: "required_review_failed";
      provider: string;
      detail: string;
    }
  | {
      reason: "required_review_missing";
    }
  | {
      reason: "blocker_policy_finding";
      rule_id: string;
      detail: string;
    }
  | {
      reason: "empty_diff";
    }
  | {
      reason: "diff_does_not_apply";
      detail: string;
    }
  | {
      reason: "coverage_below_threshold";
      measured: number;
      required: number;
    }
  | {
      reason: "repair_budget_exhausted";
      used: number;
      budget: number;
    }
  | {
      reason: "time_budget_exceeded";
      detail: string;
    }
  | {
      reason: "cancelled";
    }
  | {
      reason: "interrupted";
    }
  | {
      reason: "integration_failed";
      detail: string;
    };

export interface CandidateView {
  candidate_id: CandidateId;
  ordinal: number;
  strategy: string;
  strategy_label: string;
  status: CandidateStatus;
  status_label: string;
  branch: string;
  repairs_used: number;
  repair_budget: number;
  changed_files: number;
  changed_lines: number;
  gate_duration: DurationMs;
  score?: ScoreTuple | null;
  score_components: ScoreComponent[];
  exclusion_reasons: ExclusionReason[];
  exclusion_summaries: string[];
  rank?: number | null;
  is_winner: boolean;
  promotable: boolean;
  tests_passed?: boolean | null;
  review_passed?: boolean | null;
  line_coverage_percent?: number | null;
}
export interface ScoreTuple {
  blocker_issues: number;
  critical_issues: number;
  high_issues: number;
  medium_issues: number;
  new_security_weight: number;
  new_reliability_weight: number;
  new_maintainability_weight: number;
  coverage_rank: CoverageRank;
  test_integrity_penalty: number;
  changed_lines: number;
  repair_attempts: number;
  gate_duration_ms: number;
  candidate_id: CandidateId;
}
export interface ScoreComponent {
  label: string;
  value: string;
}

export type CommitPolicy = "manual" | "automatic" | "none";
export type AgentDriverKind =
  | "local"
  | "fake"
  | "claude_code"
  | "codex_cli"
  | "open_code"
  | "generic_process";
export type TimeoutSeconds = number;
export type NetworkPolicy = "disabled" | "loopback-only" | "approved-endpoints";
export type QualityProfile = "standard" | "strict";
export type CommandId = string;
export type CommandKind =
  | "format"
  | "lint"
  | "test"
  | "coverage"
  | "audit"
  | "secret_scan"
  | "static_analysis"
  | "policy"
  | "build";
export type ReportFormat =
  | "none"
  | "j_unit_xml"
  | "lcov"
  | "sarif"
  | "cargo_test_json"
  | "cargo_test_text"
  | "go_test_json"
  | "pytest_text"
  | "node_test_text"
  | "c_test_text"
  | "text";
export type RepositoryTrustState = "no_repository_configuration" | "trusted" | "untrusted";
export type ContentDigest = string;
export type WithheldReason =
  | "user_configuration_only"
  | "requires_repository_trust"
  | "would_weaken_policy";
export type CommandCatalogueSource =
  | {
      kind: "user_configuration";
    }
  | {
      kind: "repository_configuration";
    }
  | {
      kind: "detected";
      detail: string;
    }
  | {
      kind: "nothing_detected";
      detail: string[];
    }
  | {
      kind: "not_surveyed";
      detail: string;
    }
  | {
      kind: "declared_for_this_run";
    };

export interface EffectiveConfiguration {
  schema_version: number;
  repository_path: string;
  budgets: RunBudgets;
  commit_policy: CommitPolicy;
  agent: AgentConfiguration;
  quality: QualityConfiguration;
  git: GitConfiguration;
  commands: CommandCatalogue;
  path_policy: PathPolicy;
  redaction: RedactionConfiguration;
  retry: RetryPolicy;
  timeouts: NodeTimeouts;
  environment_allowlist: string[];
  demonstration_mode: boolean;
  repository_trust?: RepositoryTrustDecision;
  command_source?: CommandCatalogueSource;
  detection_notes?: string[];
}
export interface RunBudgets {
  candidates: number;
  max_parallel_candidates: number;
  max_repairs_per_candidate: number;
  wall_clock_seconds: number;
  max_agent_turns: number;
  max_output_bytes_per_stream: number;
  max_total_artifact_bytes: number;
}
export interface AgentConfiguration {
  driver: AgentDriverKind;
  model?: string | null;
  endpoint?: string | null;
  api_key_environment_variable?: string | null;
  executable?: string | null;
  extra_arguments: string[];
  max_turns: number;
  timeout: TimeoutSeconds;
  network: NetworkPolicy;
  fixture_script?: string | null;
}
export interface QualityConfiguration {
  profile: QualityProfile;
  minimum_line_coverage?: number | null;
  protect_existing_tests: boolean;
  sonar_scanner: SonarScannerConfiguration;
  sonar_mcp: SonarMcpConfiguration;
  ai_review: AiReviewConfiguration;
}
export interface SonarScannerConfiguration {
  enabled: boolean;
  program: string;
  arguments: string[];
  host_url: string;
  project_key?: string | null;
  token_environment_variable?: string | null;
  wait_for_quality_gate: boolean;
  timeout: TimeoutSeconds;
}
export interface SonarMcpConfiguration {
  enabled: boolean;
  program: string;
  arguments: string[];
  token_environment_variable?: string | null;
  project_key?: string | null;
  timeout: TimeoutSeconds;
}
export interface AiReviewConfiguration {
  enabled: boolean;
  advisory_only: boolean;
  gate_rules: string[];
}
export interface GitConfiguration {
  branch_prefix: string;
  author_name: string;
  include_dirty: boolean;
  require_clean_repository: boolean;
}
export interface CommandCatalogue {
  commands: CommandSpecification[];
}
export interface CommandSpecification {
  id: CommandId;
  kind: CommandKind;
  program: string;
  args: string[];
  working_subdirectory?: string | null;
  timeout: TimeoutSeconds;
  required: boolean;
  report_format: ReportFormat;
  report_path?: string | null;
  environment: [string, string][];
  success_exit_codes: number[];
}
export interface PathPolicy {
  protected_patterns: string[];
  sensitive_patterns: string[];
  approved_protected_paths: string[];
  maximum_read_bytes: number;
  maximum_write_bytes: number;
}
export interface RedactionConfiguration {
  secret_environment_variables: string[];
  additional_patterns: string[];
  redact_home_prefix: boolean;
}
export interface RetryPolicy {
  maximum_attempts: number;
  initial_delay_ms: number;
  multiplier: number;
  maximum_delay_ms: number;
}
export interface NodeTimeouts {
  agent_seconds: number;
  command_seconds: number;
  review_seconds: number;
  git_seconds: number;
}
export interface RepositoryTrustDecision {
  state: RepositoryTrustState;
  configuration_digest?: ContentDigest | null;
  withheld: WithheldSetting[];
}
export interface WithheldSetting {
  setting: string;
  reason: WithheldReason;
}

export interface CreateRunRequest {
  repository_path: string;
  task_markdown: string;
  candidate_count?: number | null;
  max_parallel_candidates?: number | null;
  max_repairs_per_candidate?: number | null;
  commit_policy?: CommitPolicy | null;
  quality_profile?: QualityProfile | null;
  minimum_line_coverage?: number | null;
  include_dirty: boolean;
  agent_driver?: string | null;
  agent_model?: string | null;
  demonstration_mode: boolean;
  wall_clock_seconds?: number | null;
  command_declarations?: CommandDeclaration[];
}
export interface CommandDeclaration {
  kind: CommandKind;
  program: string;
  args?: string[];
  timeout_seconds?: number | null;
}

export type CheckOutcome = "passed" | "warning" | "failed" | "skipped";

export interface DoctorReport {
  repository_path?: string | null;
  checks: DoctorCheck[];
  adapters: AdapterStatus[];
  ready: boolean;
  free_path_available: boolean;
}
export interface DoctorCheck {
  id: string;
  category: string;
  title: string;
  outcome: CheckOutcome;
  detail: string;
  remedy?: string | null;
}
export interface AdapterStatus {
  name: string;
  kind: string;
  available: boolean;
  version?: string | null;
  requires_paid_account: boolean;
  isolation?: string | null;
  detail: string;
}

export type EventId = string;
export type RunId = string;
export type NodeId =
  | "prepare"
  | "plan"
  | "approval"
  | "fan_out"
  | "implement_candidate"
  | "test_candidate"
  | "review_candidate"
  | "repair_candidate"
  | "join"
  | "integrate_winner"
  | "final_test"
  | "final_review"
  | "commit_approval"
  | "commit";
export type AttemptNumber = number;
export type Timestamp = string;
export type EventPayload =
  | {
      type: "run_created";
      repository_path: string;
      task_title: string;
      task_digest: ContentDigest;
      candidate_count: number;
      commit_policy: CommitPolicy;
      agent_driver: string;
      demonstration_mode: boolean;
    }
  | {
      type: "run_status_changed";
      from: RunStatus;
      to: RunStatus;
      reason?: string | null;
    }
  | {
      type: "baseline_resolved";
      baseline_commit: CommitHash;
      default_branch: string;
      dirty_snapshot: boolean;
    }
  | {
      type: "configuration_snapshotted";
      digest: ContentDigest;
      command_ids: string[];
      required_command_ids: string[];
      review_providers: string[];
    }
  | {
      type: "node_started";
      node_id: NodeId;
      candidate_id?: CandidateId | null;
      attempt: AttemptNumber;
      prompt_template_hash?: ContentDigest | null;
    }
  | {
      type: "node_succeeded";
      node_id: NodeId;
      candidate_id?: CandidateId | null;
      attempt: AttemptNumber;
      duration: DurationMs;
      next?: NodeId | null;
      result_digest: ContentDigest;
    }
  | {
      type: "node_failed";
      node_id: NodeId;
      candidate_id?: CandidateId | null;
      attempt: AttemptNumber;
      duration: DurationMs;
      failure: NodeFailure;
      next?: NodeId | null;
      result_digest: ContentDigest;
    }
  | {
      type: "node_paused";
      node_id: NodeId;
      candidate_id?: CandidateId | null;
      attempt: AttemptNumber;
      reason: string;
      result_digest: ContentDigest;
    }
  | {
      type: "node_cancelled";
      node_id: NodeId;
      candidate_id?: CandidateId | null;
      attempt: AttemptNumber;
      result_digest: ContentDigest;
    }
  | {
      type: "node_interrupted";
      node_id: NodeId;
      candidate_id?: CandidateId | null;
      attempt: AttemptNumber;
      detected_at: Timestamp;
    }
  | {
      type: "node_retry_scheduled";
      node_id: NodeId;
      candidate_id?: CandidateId | null;
      attempt: AttemptNumber;
      delay: DurationMs;
      reason: string;
    }
  | {
      type: "plan_version_written";
      version: number;
      plan_hash: ContentDigest;
      author: PlanAuthor;
      revision_note?: string | null;
      byte_length: number;
    }
  | {
      type: "plan_decision_recorded";
      approval_id: ApprovalId;
      decision: ApprovalDecision;
      plan_version: number;
      plan_hash: ContentDigest;
      local_user: string;
      note?: string | null;
    }
  | {
      type: "plan_approval_invalidated";
      previous_plan_hash: ContentDigest;
      current_plan_hash: ContentDigest;
    }
  | {
      type: "candidate_registered";
      candidate_id: CandidateId;
      ordinal: CandidateOrdinal;
      strategy: CandidateStrategy;
      branch: string;
      worktree_relative_path: string;
      repair_budget: number;
    }
  | {
      type: "candidate_status_changed";
      candidate_id: CandidateId;
      from: CandidateStatus;
      to: CandidateStatus;
      reason?: string | null;
    }
  | {
      type: "candidate_diff_recorded";
      candidate_id: CandidateId;
      diff_digest: ContentDigest;
      changed_files: number;
      changed_lines: number;
    }
  | {
      type: "candidate_repair_started";
      candidate_id: CandidateId;
      repairs_used: number;
      repair_budget: number;
      failure_fingerprint?: string | null;
    }
  | {
      type: "test_evidence_recorded";
      candidate_id?: CandidateId | null;
      node_id: NodeId;
      passed: boolean;
      commands: string[];
      failed_commands: string[];
      line_coverage_percent?: number | null;
      duration: DurationMs;
    }
  | {
      type: "review_evidence_recorded";
      candidate_id?: CandidateId | null;
      node_id: NodeId;
      passed: boolean;
      providers: string[];
      failed_providers: string[];
      blocker_issues: number;
      duration: DurationMs;
    }
  | {
      type: "candidate_scored";
      candidate_id: CandidateId;
      score: ScoreTuple;
    }
  | {
      type: "candidate_excluded";
      candidate_id: CandidateId;
      reasons: ExclusionReason[];
    }
  | {
      type: "ranking_computed";
      ranking: Ranking;
    }
  | {
      type: "winner_selected";
      candidate_id: CandidateId;
      rank: number;
    }
  | {
      type: "integration_attempted";
      candidate_id: CandidateId;
      applied: boolean;
      detail?: string | null;
    }
  | {
      type: "candidate_promotion_requested";
      previous_candidate_id: CandidateId;
      next_candidate_id?: CandidateId | null;
      reason: string;
    }
  | {
      type: "commit_approval_recorded";
      approval_id: ApprovalId;
      local_user: string;
      note?: string | null;
    }
  | {
      type: "commit_created";
      branch: BranchName;
      commit_hash: CommitHash;
      author_name: string;
      committer_name: string;
      changed_files: number;
      signed: boolean;
    }
  | {
      type: "cancellation_requested";
      requested_by: string;
      reason?: string | null;
    }
  | {
      type: "recovery_started";
      last_applied_sequence: number;
      interrupted_attempts: number;
    }
  | {
      type: "recovery_completed";
      replayed_events: number;
      repaired_projections: string[];
    }
  | {
      type: "process_supervision_recorded";
      node_id: NodeId;
      candidate_id?: CandidateId | null;
      command_id: string;
      process_id?: number | null;
      exit_code?: number | null;
      timed_out: boolean;
      children_terminated: number;
    }
  | {
      type: "artifact_stored";
      artifact_id: ContentDigest;
      label: string;
      relative_path: string;
      byte_length: number;
      truncated: boolean;
    }
  | {
      type: "run_exported";
      archive_relative_path: string;
      byte_length: number;
      redacted: boolean;
    }
  | {
      type: "diagnostic_recorded";
      level: DiagnosticLevel;
      code: string;
      message: string;
      detail?: unknown;
    };
export type RunStatus =
  | "created"
  | "validating"
  | "planning"
  | "awaiting_plan_approval"
  | "running_candidates"
  | "joining"
  | "integrating"
  | "awaiting_commit_approval"
  | "succeeded"
  | "exhausted"
  | "failed"
  | "cancelled"
  | "recovery_required";
export type CommitHash = string;
export type FailureClass =
  | "task_failure"
  | "transient_infrastructure"
  | "permanent_configuration"
  | "policy_violation"
  | "user_action_required"
  | "cancelled"
  | "internal_invariant";
export type PlanAuthor = "agent" | "human";
export type ApprovalId = string;
export type ApprovalDecision = "approved" | "revision_requested" | "rejected";
export type CandidateOrdinal = number;
export type CandidateStrategy = "minimal_patch" | "test_led" | "architecture_aware";
export type BranchName = string;
export type DiagnosticLevel = "info" | "warning" | "error";

export interface DurableEvent {
  schema_version: number;
  sequence: number;
  event_id: EventId;
  run_id: RunId;
  candidate_id?: CandidateId | null;
  node_id?: NodeId | null;
  attempt?: AttemptNumber | null;
  recorded_at: Timestamp;
  event_type: string;
  previous_hash: string;
  payload_hash: string;
  payload: EventPayload;
}
export interface NodeFailure {
  class: FailureClass;
  code: string;
  message: string;
  remedy?: string | null;
  evidence_reference?: string | null;
  fingerprint?: string | null;
}
export interface Ranking {
  entries: RankedCandidate[];
  winner?: CandidateId | null;
  rationale: string[];
}
export interface RankedCandidate {
  candidate_id: CandidateId;
  eligible: boolean;
  score?: ScoreTuple | null;
  exclusion_reasons: ExclusionReason[];
  rank?: number | null;
}

export type NodeStatus = "succeeded" | "failed" | "paused" | "cancelled" | "interrupted";
export interface NodeResult {
  schema_version: number;
  run_id: RunId;
  candidate_id?: CandidateId | null;
  node_id: NodeId;
  attempt: AttemptNumber;
  status: NodeStatus;
  started_at: Timestamp;
  finished_at: Timestamp;
  duration_ms: DurationMs;
  next?: NodeId | null;
  state_patch: StatePatch;
  artifacts: ArtifactReference[];
  failure?: NodeFailure | null;
  metrics: unknown;
  warnings: string[];
}
export interface StatePatch {
  run_status?: RunStatus | null;
  candidate_status?: CandidateStatus | null;
  plan_version?: number | null;
  baseline_commit?: CommitHash | null;
  changed_lines?: number | null;
  changed_files?: number | null;
  diff_digest?: ContentDigest | null;
  repairs_used?: number | null;
  gate_duration_ms?: number | null;
  failure_fingerprint?: string | null;
  exclusion_reasons?: ExclusionReason[] | null;
  score?: ScoreTuple | null;
  winner?: CandidateId | null;
  commit_hash?: CommitHash | null;
  branch?: BranchName | null;
  promotable?: boolean | null;
  integration_attempted?: boolean | null;
}
export interface ArtifactReference {
  id: ContentDigest;
  label: string;
  relative_path: string;
  media_type: string;
  byte_length: number;
  truncated: boolean;
}
export type QualityGateOutcome = "passed" | "failed" | "not_applicable";
export type IssueCategory =
  | "security"
  | "reliability"
  | "maintainability"
  | "coverage"
  | "formatting"
  | "policy"
  | "test_integrity"
  | "dependency"
  | "secret";
export type IssueSeverity = "info" | "low" | "medium" | "high" | "critical" | "blocker";
export interface ReviewReport {
  schema_version: number;
  provider: string;
  required: boolean;
  advisory: boolean;
  passed: boolean;
  quality_gate: QualityGateOutcome;
  issues: ReviewIssue[];
  metrics: ReviewMetrics;
  artifacts: ReviewArtifactReference[];
  started_at: Timestamp;
  finished_at: Timestamp;
  failure_summary?: string | null;
}
export interface ReviewIssue {
  provider: string;
  fingerprint: string;
  rule_id: string;
  category: IssueCategory;
  severity: IssueSeverity;
  file?: string | null;
  line?: number | null;
  column?: number | null;
  message: string;
  help_reference?: string | null;
  is_new: boolean;
}
export interface ReviewMetrics {
  line_coverage_percent?: number | null;
  branch_coverage_percent?: number | null;
  changed_lines?: number | null;
  changed_files?: number | null;
  analysed_files?: number | null;
  duplicated_lines?: number | null;
}
export interface ReviewArtifactReference {
  label: string;
  relative_path: string;
  media_type: string;
  digest: ContentDigest;
  byte_length: number;
}

export type NodeAttemptStatus =
  | "started"
  | "succeeded"
  | "failed"
  | "paused"
  | "cancelled"
  | "interrupted";
export type GraphNodeState = "pending" | "active" | "succeeded" | "failed" | "paused" | "skipped";
export type TimelineLevel = "information" | "success" | "warning" | "failure";

export interface RunDetail {
  summary: RunSummary;
  projection: RunProjection;
  candidates: CandidateView[];
  graph: GraphView;
  timeline: TimelineEntry[];
  metrics: RunMetrics;
  ranking_rationale: string[];
  integration_detail?: string | null;
}
export interface RunSummary {
  run_id: RunId;
  status: RunStatus;
  status_label: string;
  repository_path: string;
  task_title: string;
  created_at: Timestamp;
  updated_at: Timestamp;
  elapsed: DurationMs;
  current_nodes: NodeId[];
  candidate_progress: CandidateProgress;
  winner?: CandidateId | null;
  last_event_summary?: string | null;
  demonstration_mode: boolean;
  commit_hash?: string | null;
  branch?: string | null;
  plan_version?: number | null;
  plan_approved: boolean;
  commit_approved: boolean;
  recovery_reason?: string | null;
}
export interface CandidateProgress {
  total: number;
  eligible: number;
  ineligible: number;
  active: number;
  pending: number;
}
export interface RunProjection {
  schema_version: number;
  run_id: RunId;
  created_at: Timestamp;
  updated_at: Timestamp;
  last_event_sequence: number;
  last_event_hash: string;
  status: RunStatus;
  repository_path: string;
  task_title: string;
  task_digest: ContentDigest;
  candidate_count: number;
  commit_policy: CommitPolicy;
  agent_driver: string;
  demonstration_mode: boolean;
  baseline_commit?: CommitHash | null;
  default_branch?: string | null;
  dirty_snapshot: boolean;
  configuration_digest?: ContentDigest | null;
  command_ids: string[];
  required_command_ids: string[];
  review_providers: string[];
  plan: PlanHistory;
  candidates: CandidateRecord[];
  attempts: NodeAttemptRecord[];
  ranking?: Ranking | null;
  winner?: CandidateId | null;
  integration: IntegrationState;
  commit?: CommitRecord | null;
  commit_approved: boolean;
  cancellation_requested: boolean;
  recovery_reason?: string | null;
  metrics: RunMetrics;
  last_event_summary?: string | null;
  export_paths: string[];
}
export interface PlanHistory {
  versions: PlanVersion[];
  approval?: PlanApproval | null;
}
export interface PlanVersion {
  version: number;
  hash: ContentDigest;
  created_at: Timestamp;
  author: PlanAuthor;
  revision_note?: string | null;
  byte_length: number;
}
export interface PlanApproval {
  id: ApprovalId;
  decision: ApprovalDecision;
  plan_version: number;
  plan_hash: ContentDigest;
  decided_at: Timestamp;
  local_user: string;
  note?: string | null;
}
export interface CandidateRecord {
  id: CandidateId;
  ordinal: CandidateOrdinal;
  strategy: CandidateStrategy;
  status: CandidateStatus;
  baseline_commit: CommitHash;
  branch: string;
  worktree_relative_path: string;
  repairs_used: number;
  repair_budget: number;
  changed_lines: number;
  changed_files: number;
  diff_digest?: ContentDigest | null;
  started_at?: Timestamp | null;
  finished_at?: Timestamp | null;
  gate_duration: DurationMs;
  last_failure_fingerprint?: string | null;
  repeated_fingerprint_count: number;
  score?: ScoreTuple | null;
  exclusion_reasons: ExclusionReason[];
  promotable: boolean;
  integration_attempted: boolean;
}
export interface NodeAttemptRecord {
  node_id: NodeId;
  candidate_id?: CandidateId | null;
  attempt: AttemptNumber;
  status: NodeAttemptStatus;
  started_at: Timestamp;
  finished_at?: Timestamp | null;
  duration: DurationMs;
  failure_summary?: string | null;
  failure_class?: FailureClass | null;
  next?: NodeId | null;
  sequence: number;
}
export interface IntegrationState {
  attempted_candidates: CandidateId[];
  applied_candidate?: CandidateId | null;
  final_tests_passed?: boolean | null;
  final_review_passed?: boolean | null;
  last_detail?: string | null;
}
export interface CommitRecord {
  branch: BranchName;
  commit_hash: CommitHash;
  author_name: string;
  committer_name: string;
  changed_files: number;
  signed: boolean;
}
export interface RunMetrics {
  node_executions: number;
  node_failures: number;
  automatic_retries: number;
  repair_loops: number;
  candidate_duration_ms: number;
  test_duration_ms: number;
  review_duration_ms: number;
  agent_duration_ms: number;
  changed_lines: number;
  events_recorded: number;
  artifacts_stored: number;
  processes_supervised: number;
  processes_timed_out: number;
  reported_input_tokens?: number | null;
  reported_output_tokens?: number | null;
  reported_cost_minor_units?: number | null;
  reported_cost_currency?: string | null;
}
export interface GraphView {
  nodes: GraphNodeView[];
  edges: GraphEdgeView[];
}
export interface GraphNodeView {
  id: string;
  label: string;
  scope: string;
  class: string;
  state: GraphNodeState;
  attempts: number;
  total_duration_ms: number;
}
export interface GraphEdgeView {
  from: string;
  to: string;
  label: string;
  traversed: boolean;
}
export interface TimelineEntry {
  sequence: number;
  recorded_at: Timestamp;
  node_id?: NodeId | null;
  node_label?: string | null;
  candidate_id?: CandidateId | null;
  attempt?: number | null;
  event_type: string;
  summary: string;
  duration?: DurationMs | null;
  level: TimelineLevel;
}
