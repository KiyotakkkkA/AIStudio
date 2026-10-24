import type { ReactNode } from "react";
import { observer } from "mobx-react-lite";
import { mdiClose } from "@mdi/js";
import IconButton from "../../ui/atoms/buttons/IconButton";
import type ChatStore from "./ChatStore";

export default observer(function SidePanelFrame({
  store,
  title,
  titleHint,
  actions,
  children,
}: {
  readonly store: ChatStore;
  readonly title: string;
  readonly titleHint?: string;
  readonly actions?: ReactNode;
  readonly children: ReactNode;
}) {
  return (
    <aside
      aria-label={title}
      className={`flex w-76 shrink-0 flex-col bg-main-800 max-[1250px]:w-60 ${store.panelOpen ? "max-[1100px]:absolute max-[1100px]:inset-y-0 max-[1100px]:right-0 max-[1100px]:z-10 max-[1100px]:w-76 max-[1100px]:shadow-pop" : "hidden"}`}
    >
      <div className="flex h-14 shrink-0 items-center gap-1 pr-2.5 pl-4">
        <h2
          title={titleHint}
          className="min-w-0 flex-1 truncate text-[14px] font-medium text-main-100"
        >
          {title}
        </h2>
        {actions}
        <span className="min-[1101px]:hidden">
          <IconButton
            aria-label={`Закрыть: ${title}`}
            label="Закрыть"
            tooltip="bottom-left"
            path={mdiClose}
            iconSize={17}
            onClick={store.closePanel}
          />
        </span>
      </div>
      {children}
    </aside>
  );
});
