import { useCallback, useMemo } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { DEFAULT_ROUTE, isRoutePath, type RoutePath } from "../app/routes";

export interface AppNavigation {
  readonly current: RoutePath;
  readonly pathname: string;
  isActive(path: string): boolean;
  go(path: string): void;
  replace(path: string): void;
}

export function useAppNavigation(): AppNavigation {
  const navigate = useNavigate();
  const { pathname } = useLocation();

  const current = isRoutePath(pathname) ? pathname : DEFAULT_ROUTE;

  const isActive = useCallback((path: string) => pathname === path, [pathname]);

  const go = useCallback(
    (path: string) => {
      if (path !== pathname) void navigate(path);
    },
    [navigate, pathname],
  );

  const replace = useCallback(
    (path: string) => {
      if (path !== pathname) void navigate(path, { replace: true });
    },
    [navigate, pathname],
  );

  return useMemo(
    () => ({ current, pathname, isActive, go, replace }),
    [current, pathname, isActive, go, replace],
  );
}

export default useAppNavigation;
