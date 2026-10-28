import { observer } from "mobx-react-lite";
import { useEffect } from "react";
import { useStore } from "../../stores/useStore";
import ToolchainCard from "./ToolchainCard";
import type { ToolchainAction } from "./toolchainPresentation";
import ListRowSkeleton from "../../ui/molecules/ListRowSkeleton";
import { LoadingStatus } from "../../ui/molecules/LoadingSkeleton";

function ComponentsTab() {
  const { toolchain } = useStore();

  useEffect(() => {
    void toolchain.mount();
    return () => toolchain.dispose();
  }, [toolchain]);

  return (
    <div className="flex flex-col gap-3">
      {toolchain.error === null ? null : (
        <p role="alert" className="m-0 text-[12px] text-err">
          {toolchain.error}
        </p>
      )}
      {toolchain.loading && toolchain.ordered.length === 0 ? (
        <LoadingStatus label="Загрузка…" className="flex flex-col gap-3">
          <ListRowSkeleton />
          <ListRowSkeleton />
        </LoadingStatus>
      ) : null}
      <div className="flex flex-col gap-3">
        {toolchain.ordered.map((dto) => (
          <ToolchainCard
            key={dto.runtime}
            dto={dto}
            busy={toolchain.busy[dto.runtime]}
            error={toolchain.errors[dto.runtime]}
            onAction={(action: ToolchainAction) => {
              void toolchain[action](dto.runtime);
            }}
          />
        ))}
      </div>
    </div>
  );
}

export default observer(ComponentsTab);
