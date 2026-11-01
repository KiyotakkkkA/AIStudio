import { observer } from "mobx-react-lite";
import type { BackgroundShellId } from "@zvs/shared";
import StatusDot from "../../ui/atoms/StatusDot";
import { useStore } from "../../stores/useStore";
import { IN_BACKGROUND, OUTPUT, STOP, STOPPING, statusLabel } from "./backgroundShellsCopy";

export default observer(function BackgroundShellLine({ id }: { readonly id: BackgroundShellId }) {
  const { chat } = useStore();
  const store = chat.backgroundShells;
  const shell = store.find(id);
  if (shell === undefined) {
    return <p className="m-0 font-mono text-[11.5px] text-main-300">{IN_BACKGROUND}</p>;
  }
  const running = shell.status === "running";
  const stopping = store.isStopping(id);
  const label = running ? IN_BACKGROUND : statusLabel(shell.status, shell.exitCode);
  return (
    <div className="flex items-center gap-2 text-[11.5px]">
      <StatusDot tone={running ? "ok" : "idle"} title={label} />
      <span className="min-w-0 flex-1 font-mono text-main-300 tabular-nums">{label}</span>
      <button
        type="button"
        onClick={() => chat.openShellOutput(id)}
        className="rounded-control px-2 py-0.5 text-[12px] text-accent-medium hover:bg-main-750"
      >
        {OUTPUT}
      </button>
      {running ? (
        <button
          type="button"
          disabled={stopping}
          onClick={() => void store.stop(id)}
          className="rounded-control px-2 py-0.5 text-[12px] text-err hover:bg-main-750 disabled:opacity-50"
        >
          {stopping ? STOPPING : STOP}
        </button>
      ) : null}
    </div>
  );
});
