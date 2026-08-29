import {
  createRootRoute,
  createRoute,
  createRouter,
  Outlet,
  useParams,
} from "@tanstack/react-router";

import { AppShell } from "./AppShell";
import { DashboardPage } from "@/features/dashboard/DashboardPage";
import { NewRunPage } from "@/features/new-run/NewRunPage";
import { RunDetailPage } from "@/features/run-detail/RunDetailPage";
import { PlanPage } from "@/features/plan/PlanPage";
import { CandidateComparisonPage } from "@/features/candidates/CandidateComparisonPage";
import { DoctorPage } from "@/features/doctor/DoctorPage";
import { SettingsPage } from "@/features/settings/SettingsPage";
import { DocumentationPage } from "@/features/documentation/DocumentationPage";
import { EmptyState } from "@/components/StateViews";

const rootRoute = createRootRoute({
  component: () => (
    <AppShell>
      <Outlet />
    </AppShell>
  ),
  notFoundComponent: () => (
    <div className="p-10">
      <EmptyState
        title="That route does not exist"
        description="Use the navigation to reach the dashboard, a run, the doctor or the settings."
      />
    </div>
  ),
});

const dashboardRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/",
  component: DashboardPage,
});

const newRunRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/new",
  component: NewRunPage,
});

const runDetailRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/runs/$runId",
  component: function RunDetailRouteComponent() {
    const { runId } = useParams({ from: "/runs/$runId" });
    return <RunDetailPage runId={runId} />;
  },
});

const planRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/runs/$runId/plan",
  component: function PlanRouteComponent() {
    const { runId } = useParams({ from: "/runs/$runId/plan" });
    return <PlanPage runId={runId} />;
  },
});

const candidatesRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/runs/$runId/candidates",
  component: function CandidatesRouteComponent() {
    const { runId } = useParams({ from: "/runs/$runId/candidates" });
    return <CandidateComparisonPage runId={runId} />;
  },
});

const doctorRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/doctor",
  component: DoctorPage,
});

const settingsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/settings",
  component: SettingsPage,
});

const documentationRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/documentation",
  component: DocumentationPage,
});

const routeTree = rootRoute.addChildren([
  dashboardRoute,
  newRunRoute,
  runDetailRoute,
  planRoute,
  candidatesRoute,
  doctorRoute,
  settingsRoute,
  documentationRoute,
]);

export const router = createRouter({
  routeTree,
  defaultPreload: "intent",
  defaultPreloadStaleTime: 0,
});

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
