import BrowserSurface from "../features/browser/BrowserSurface";

export default function StandaloneBrowserPage() {
  return (
    <div className="flex size-full min-h-0 flex-col overflow-hidden bg-main-950">
      <BrowserSurface />
    </div>
  );
}
