import { lazy, Suspense, type ComponentType } from "react";
import { createHashRouter, Navigate, type RouteObject } from "react-router-dom";
import AppFrame from "./AppFrame";
import RouteFallback from "./RouteFallback";
import { DEFAULT_ROUTE, ROUTES } from "./routes";

const SecretsPage = lazy(() => import("../pages/SecretsPage"));
const ProvidersPage = lazy(() => import("../pages/ProvidersPage"));
const VectorStoresPage = lazy(() => import("../pages/VectorStoresPage"));
const ChatPage = lazy(() => import("../pages/ChatPage"));
const AgenticBuildPage = lazy(() => import("../pages/AgenticBuildPage"));
const ScenariosPage = lazy(() => import("../pages/ScenariosPage"));
const SkillsPage = lazy(() => import("../pages/SkillsPage"));
const ConnectionsPage = lazy(() => import("../pages/ConnectionsPage"));
const IntegrationsPage = lazy(() => import("../pages/IntegrationsPage"));
const ToolsPage = lazy(() => import("../pages/ToolsPage"));
const TasksPage = lazy(() => import("../pages/TasksPage"));
const DownloadsPage = lazy(() => import("../pages/DownloadsPage"));
const RunsPage = lazy(() => import("../pages/RunsPage"));
const SettingsPage = lazy(() => import("../pages/SettingsPage"));
const BrowserPage = lazy(() => import("../pages/BrowserPage"));

const page = (path: string, Component: ComponentType): RouteObject => ({
  path,
  element: (
    <Suspense fallback={<RouteFallback />}>
      <Component />
    </Suspense>
  ),
});

export const routes: RouteObject[] = [
  {
    path: "/",
    element: <AppFrame />,
    children: [
      { index: true, element: <Navigate to={DEFAULT_ROUTE} replace /> },
      page(ROUTES.secrets, SecretsPage),
      page(ROUTES.providers, ProvidersPage),
      page(ROUTES.vectorStores, VectorStoresPage),
      page(ROUTES.chat, ChatPage),
      page(ROUTES.agenticBuild, AgenticBuildPage),
      page(ROUTES.scenarios, ScenariosPage),
      page(ROUTES.skills, SkillsPage),
      page(ROUTES.connections, ConnectionsPage),
      page(ROUTES.integrations, IntegrationsPage),
      page(ROUTES.tools, ToolsPage),
      page(ROUTES.tasks, TasksPage),
      page(ROUTES.downloads, DownloadsPage),
      page(ROUTES.runs, RunsPage),
      page(ROUTES.settings, SettingsPage),
      page(ROUTES.browser, BrowserPage),
      { path: "*", element: <Navigate to={DEFAULT_ROUTE} replace /> },
    ],
  },
];

export function createAppRouter() {
  return createHashRouter(routes);
}
