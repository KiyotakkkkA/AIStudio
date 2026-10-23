import { observer } from "mobx-react-lite";
import { useEffect } from "react";
import { useStore } from "../../stores/useStore";
import ToolchainCard from "./ToolchainCard";
import type { ToolchainAction } from "./toolchainPresentation";

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
      {toolchain.loading ? <p className="m-0 text-[12px] text-main-400">Загрузка…</p> : null}
      <div className="grid gap-3 lg:grid-cols-2">
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
