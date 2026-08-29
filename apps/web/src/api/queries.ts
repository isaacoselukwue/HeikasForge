import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type { UseMutationResult, UseQueryResult } from "@tanstack/react-query";

import { api } from "./client";
import type {
  AcknowledgementResponse,
  ConfigurationResponse,
  EventPage,
  ExportResponse,
  GraphDefinition,
  HealthResponse,
  LogPage,
  PlanResponse,
} from "./client";
import type {
  CandidateView,
  CreateRunRequest,
  DoctorReport,
  RunDetail,
  RunSummary,
  TimelineEntry,
} from "@/generated/api-types";

export const queryKeys = {
  health: ["health"] as const,
  configuration: ["configuration"] as const,
  graph: ["graph"] as const,
  runs: ["runs"] as const,
  run: (runId: string) => ["run", runId] as const,
  plan: (runId: string) => ["plan", runId] as const,
  candidates: (runId: string) => ["candidates", runId] as const,
  timeline: (runId: string) => ["timeline", runId] as const,
  events: (runId: string) => ["events", runId] as const,
  logs: (runId: string) => ["logs", runId] as const,
  doctor: (repository: string | null) => ["doctor", repository] as const,
  candidateDiff: (runId: string, candidateId: string) =>
    ["candidate-diff", runId, candidateId] as const,
  integrationDiff: (runId: string) => ["integration-diff", runId] as const,
};

export function useHealth(): UseQueryResult<HealthResponse> {
  return useQuery({
    queryKey: queryKeys.health,
    queryFn: api.health,
    refetchInterval: 15_000,
  });
}

export function useConfiguration(): UseQueryResult<ConfigurationResponse> {
  return useQuery({ queryKey: queryKeys.configuration, queryFn: api.configuration });
}

export function useGraphDefinition(): UseQueryResult<GraphDefinition> {
  return useQuery({ queryKey: queryKeys.graph, queryFn: api.graph, staleTime: Infinity });
}

export function useRuns(): UseQueryResult<RunSummary[]> {
  return useQuery({
    queryKey: queryKeys.runs,
    queryFn: async () => (await api.listRuns()).runs,
    refetchInterval: 5_000,
  });
}

export function useRunDetail(runId: string): UseQueryResult<RunDetail> {
  return useQuery({
    queryKey: queryKeys.run(runId),
    queryFn: () => api.runDetail(runId),
  });
}

export function usePlan(runId: string): UseQueryResult<PlanResponse> {
  return useQuery({ queryKey: queryKeys.plan(runId), queryFn: () => api.plan(runId) });
}

export function useCandidates(runId: string): UseQueryResult<CandidateView[]> {
  return useQuery({
    queryKey: queryKeys.candidates(runId),
    queryFn: async () => (await api.candidates(runId)).candidates,
  });
}

export function useTimeline(runId: string): UseQueryResult<TimelineEntry[]> {
  return useQuery({
    queryKey: queryKeys.timeline(runId),
    queryFn: async () => (await api.timeline(runId)).entries,
  });
}

export function useEvents(runId: string): UseQueryResult<EventPage> {
  return useQuery({
    queryKey: queryKeys.events(runId),
    queryFn: () => api.events(runId, 0, 2000),
  });
}

export function useLogs(runId: string): UseQueryResult<LogPage> {
  return useQuery({
    queryKey: queryKeys.logs(runId),
    queryFn: () => api.logs(runId, 0, 1000),
    refetchInterval: 4_000,
  });
}

export function useDoctor(
  repository: string | null,
  enabled: boolean,
): UseQueryResult<DoctorReport> {
  return useQuery({
    queryKey: queryKeys.doctor(repository),
    queryFn: () => api.doctor(repository),
    enabled,
  });
}

export function useCandidateDiff(
  runId: string,
  candidateId: string | null,
): UseQueryResult<string> {
  return useQuery({
    queryKey: queryKeys.candidateDiff(runId, candidateId ?? "none"),
    queryFn: () => api.candidateDiff(runId, candidateId ?? ""),
    enabled: candidateId !== null,
  });
}

export function useIntegrationDiff(runId: string, enabled: boolean): UseQueryResult<string> {
  return useQuery({
    queryKey: queryKeys.integrationDiff(runId),
    queryFn: () => api.integrationDiff(runId),
    enabled,
  });
}

export function useCreateRun(): UseMutationResult<{ run_id: string }, Error, CreateRunRequest> {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (payload: CreateRunRequest) => api.createRun(payload),
    onSuccess: () => {
      void client.invalidateQueries({ queryKey: queryKeys.runs });
    },
  });
}

export interface RunAction {
  runId: string;
}

export function useApprovePlan(): UseMutationResult<
  AcknowledgementResponse,
  Error,
  RunAction & { markdown: string | null; note: string | null }
> {
  const client = useQueryClient();
  return useMutation({
    mutationFn: ({ runId, markdown, note }) => api.approvePlan(runId, markdown, note),
    onSuccess: (_data, variables) => {
      void client.invalidateQueries({ queryKey: queryKeys.plan(variables.runId) });
      void client.invalidateQueries({ queryKey: queryKeys.run(variables.runId) });
      void client.invalidateQueries({ queryKey: queryKeys.runs });
    },
  });
}

export function useUpdatePlan(): UseMutationResult<
  { version: number; approval_invalidated: boolean },
  Error,
  RunAction & { markdown: string }
> {
  const client = useQueryClient();
  return useMutation({
    mutationFn: ({ runId, markdown }) => api.updatePlan(runId, markdown),
    onSuccess: (_data, variables) => {
      void client.invalidateQueries({ queryKey: queryKeys.plan(variables.runId) });
      void client.invalidateQueries({ queryKey: queryKeys.run(variables.runId) });
    },
  });
}

export function useRevisePlan(): UseMutationResult<
  AcknowledgementResponse,
  Error,
  RunAction & { note: string }
> {
  const client = useQueryClient();
  return useMutation({
    mutationFn: ({ runId, note }) => api.revisePlan(runId, note),
    onSuccess: (_data, variables) => {
      void client.invalidateQueries({ queryKey: queryKeys.plan(variables.runId) });
      void client.invalidateQueries({ queryKey: queryKeys.run(variables.runId) });
    },
  });
}

export function useRejectPlan(): UseMutationResult<
  AcknowledgementResponse,
  Error,
  RunAction & { reason: string | null }
> {
  const client = useQueryClient();
  return useMutation({
    mutationFn: ({ runId, reason }) => api.rejectPlan(runId, reason),
    onSuccess: (_data, variables) => {
      void client.invalidateQueries({ queryKey: queryKeys.run(variables.runId) });
      void client.invalidateQueries({ queryKey: queryKeys.runs });
    },
  });
}

export function useApproveCommit(): UseMutationResult<
  AcknowledgementResponse,
  Error,
  RunAction & { note: string | null }
> {
  const client = useQueryClient();
  return useMutation({
    mutationFn: ({ runId, note }) => api.approveCommit(runId, note),
    onSuccess: (_data, variables) => {
      void client.invalidateQueries({ queryKey: queryKeys.run(variables.runId) });
      void client.invalidateQueries({ queryKey: queryKeys.runs });
    },
  });
}

export function useCancelRun(): UseMutationResult<
  AcknowledgementResponse,
  Error,
  RunAction & { reason: string | null }
> {
  const client = useQueryClient();
  return useMutation({
    mutationFn: ({ runId, reason }) => api.cancelRun(runId, reason),
    onSuccess: (_data, variables) => {
      void client.invalidateQueries({ queryKey: queryKeys.run(variables.runId) });
      void client.invalidateQueries({ queryKey: queryKeys.runs });
    },
  });
}

export function useResumeRun(): UseMutationResult<AcknowledgementResponse, Error, RunAction> {
  const client = useQueryClient();
  return useMutation({
    mutationFn: ({ runId }) => api.resumeRun(runId),
    onSuccess: (_data, variables) => {
      void client.invalidateQueries({ queryKey: queryKeys.run(variables.runId) });
      void client.invalidateQueries({ queryKey: queryKeys.runs });
    },
  });
}

export function useExportRun(): UseMutationResult<
  ExportResponse,
  Error,
  RunAction & { includeWorktrees: boolean }
> {
  return useMutation({
    mutationFn: ({ runId, includeWorktrees }) => api.exportRun(runId, includeWorktrees),
  });
}
