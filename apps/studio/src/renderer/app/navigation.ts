import {
  mdiBookOpenOutline,
  mdiChatOutline,
  mdiClockOutline,
  mdiCreationOutline,
  mdiDatabaseOutline,
  mdiEmailOutline,
  mdiGraphOutline,
  mdiHistory,
  mdiKeyOutline,
  mdiLayersOutline,
  mdiPowerPlugOutline,
  mdiTrayArrowDown,
  mdiTuneVariant,
  mdiWeb,
  mdiWrenchOutline,
} from "@mdi/js";
import type { NavGroupModel, RailIdentity } from "../ui/organisms/NavRailTypes";
import { ROUTES } from "./routes";

export const BROWSER_NAV_ID = "browser";

export const NAV_GROUPS: readonly NavGroupModel[] = [
  {
    id: "workspace",
    label: "Рабочее пространство",
    items: [
      { id: "chat", label: "Чат", icon: mdiChatOutline, path: ROUTES.chat },
      {
        id: "agentic-build",
        label: "Агенты",
        icon: mdiCreationOutline,
        path: ROUTES.agenticBuild,
      },
      { id: "providers", label: "Провадеры", icon: mdiLayersOutline, path: ROUTES.providers },
      { id: "scenarios", label: "Сценарии", icon: mdiGraphOutline, path: ROUTES.scenarios },
    ],
  },
  {
    id: "storage",
    label: "Хранилище",
    items: [
      { id: "secrets", label: "Секреты", icon: mdiKeyOutline, path: ROUTES.secrets },
      {
        id: "vector-stores",
        label: "Векторные хранилища",
        icon: mdiDatabaseOutline,
        path: ROUTES.vectorStores,
      },
    ],
  },
  {
    id: "extend",
    label: "Расширение",
    items: [
      { id: "skills", label: "Навыки", icon: mdiBookOpenOutline, path: ROUTES.skills },
      {
        id: "connections",
        label: "Коннекторы",
        icon: mdiPowerPlugOutline,
        path: ROUTES.connections,
      },
      {
        id: "integrations",
        label: "Интеграции",
        icon: mdiEmailOutline,
        path: ROUTES.integrations,
      },
      { id: "tools", label: "Инструменты", icon: mdiWrenchOutline, path: ROUTES.tools },
      { id: BROWSER_NAV_ID, label: "Браузер", icon: mdiWeb, external: true },
    ],
  },
  {
    id: "system",
    label: "Система",
    items: [
      { id: "tasks", label: "Задачи", icon: mdiClockOutline, path: ROUTES.tasks },
      { id: "downloads", label: "Загрузки", icon: mdiTrayArrowDown, path: ROUTES.downloads },
      { id: "runs", label: "Запуски и логи", icon: mdiHistory, path: ROUTES.runs },
      { id: "settings", label: "Настройки", icon: mdiTuneVariant, path: ROUTES.settings },
    ],
  },
];

export const RAIL_IDENTITY: RailIdentity = {
  initials: "ZV",
  name: "Локальный профиль",
  hint: "Локальное пространство · зашифровано",
};
