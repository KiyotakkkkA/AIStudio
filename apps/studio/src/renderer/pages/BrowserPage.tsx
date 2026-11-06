import { observer } from "mobx-react-lite";
import { Navigate } from "react-router-dom";
import BrowserSurface from "../features/browser/BrowserSurface";
import { ROUTES } from "../app/routes";
import { useStore } from "../stores/useStore";

export default observer(function BrowserPage() {
  const { browserSettings } = useStore();
  if (!browserSettings.enabled) return <Navigate to={ROUTES.chat} replace />;
  return <BrowserSurface />;
});
