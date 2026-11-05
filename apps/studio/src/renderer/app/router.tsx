import { lazy, Suspense, type ComponentType } from "react";
import { createHashRouter, Navigate, type RouteObject } from "react-router-dom";
import AppFrame from "./AppFrame";
import RouteFallback from "./RouteFallback";
import { DEFAULT_ROUTE, ROUTES } from "./routes";

const SecretsPage = lazy(() => import("../pages/SecretsPage"));
const ProvidersPage = lazy(() => import("../pages/ProvidersPage"));
const VectorStoresPage = lazy(() => import("../pages/VectorStoresPage"));
const GalleryPage = lazy(() => import("../pages/GalleryPage"));
const ChatPage = lazy(() => import("../pages/ChatPage"));
const AgenticBuildPage = lazy(() => import("../pages/AgenticBuildPage"));
const ScenariosPage = lazy(() => import("../pages/ScenariosPage"));
const SkillsPage = lazy(() => import("../pages/SkillsPage"));
const ConnectionsPage = lazy(() => import("../pages/ConnectionsPage"));
const PluginsPage = lazy(() => import("../pages/PluginsPage"));
const ToolsPage = lazy(() => import("../pages/ToolsPage"));
const TasksPage = lazy(() => import("../pages/TasksPage"));
const DownloadsPage = lazy(() => import("../pages/DownloadsPage"));
const RunsPage = lazy(() => import("../pages/RunsPage"));
const PermissionsPage = lazy(() => import("../pages/PermissionsPage"));
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
      page(ROUTES.gallery, GalleryPage),
      page(ROUTES.chat, ChatPage),
      page(ROUTES.agenticBuild, AgenticBuildPage),
      page(ROUTES.scenarios, ScenariosPage),
      page(ROUTES.skills, SkillsPage),
      page(ROUTES.connections, ConnectionsPage),
      page(`${ROUTES.plugins}/:kind?/:accountId?`, PluginsPage),
      page(ROUTES.tools, ToolsPage),
      page(ROUTES.tasks, TasksPage),
      page(ROUTES.downloads, DownloadsPage),
      page(ROUTES.runs, RunsPage),
      page(ROUTES.permissions, PermissionsPage),
      page(ROUTES.settings, SettingsPage),
      page(ROUTES.browser, BrowserPage),
      { path: "*", element: <Navigate to={DEFAULT_ROUTE} replace /> },
    ],
  },
];

export function createAppRouter() {
  return createHashRouter(routes);
}
