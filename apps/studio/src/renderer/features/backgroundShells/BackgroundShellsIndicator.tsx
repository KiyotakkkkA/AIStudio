import { observer } from "mobx-react-lite";
import { Dropdown, ScrollArea, Tooltip } from "@kiyotakkkka/zvs-uikit-lib";
import { mdiCogPlayOutline } from "@mdi/js";
import type { BackgroundShellDto, BackgroundShellId } from "@zvs/shared";
import Icon from "../../ui/atoms/Icon";
import StatusDot from "../../ui/atoms/StatusDot";
import type BackgroundShellStore from "./BackgroundShellStore";
import {
  FOLDER,
  LIST_LABEL,
  OUTPUT,
  RUN_TIME,
  STOP,
  STOPPING,
  commandText,
  durationText,
  runningCount,
  statusLabel,
} from "./backgroundShellsCopy";

const ShellRow = observer(function ShellRow({
  store,
  shell,
  onOutput,
}: {
  readonly store: BackgroundShellStore;
  readonly shell: BackgroundShellDto;
  readonly onOutput: (id: BackgroundShellId) => void;
}) {
  const running = shell.status === "running";
  const stopping = store.isStopping(shell.id);
  return (
    <li className="flex flex-col gap-1.5 px-3 py-2.5">
      <div className="flex min-w-0 items-center gap-2">
        <StatusDot
          tone={running ? "ok" : "idle"}
          title={statusLabel(shell.status, shell.exitCode)}
        />
        <span className="min-w-0 flex-1 truncate font-mono text-[12.5px] text-main-50">
          {commandText(shell)}
        </span>
      </div>
      <dl className="m-0 grid grid-cols-[auto_1fr] gap-x-2 gap-y-0.5 text-[11.5px]">
        <dt className="text-main-500">{FOLDER}</dt>
        <dd className="m-0 truncate text-main-300" title={shell.cwd}>
          {shell.cwd}
        </dd>
        <dt className="text-main-500">{RUN_TIME}</dt>
        <dd className="m-0 text-main-300">{durationText(store.elapsed(shell))}</dd>
      </dl>
      <div className="flex items-center justify-between gap-2">
        <span className="text-[11.5px] text-main-400">
          {statusLabel(shell.status, shell.exitCode)}
        </span>
        <div className="flex items-center gap-1">
          <button
            type="button"
            onClick={() => onOutput(shell.id)}
            className="rounded-control px-2 py-0.5 text-[12px] text-accent-medium hover:bg-main-800"
          >
            {OUTPUT}
          </button>
          {running ? (
            <button
              type="button"
              disabled={stopping}
              onClick={() => void store.stop(shell.id)}
              className="rounded-control px-2 py-0.5 text-[12px] text-err hover:bg-main-800 disabled:opacity-50"
            >
              {stopping ? STOPPING : STOP}
            </button>
          ) : null}
        </div>
      </div>
    </li>
  );
});

export default observer(function BackgroundShellsIndicator({
  store,
  onOutput,
}: {
  readonly store: BackgroundShellStore;
  readonly onOutput: (id: BackgroundShellId) => void;
}) {
  const running = store.running;
  if (running.length === 0) return null;
  const label = runningCount(running.length);
  return (
    <Dropdown menuWidth={340} menuPlacement="bottom-right">
      <Tooltip label={label} placement="bottom-center" className="">
        <Dropdown.Trigger
          rounded=""
          className="flex h-7 flex-none cursor-auto items-center gap-1 rounded-control border-none bg-accent-soft px-2 text-[12px] font-medium text-accent-medium hover:text-accent-light"
          icon={<Icon path={mdiCogPlayOutline} size={15} />}
        >
          <span aria-hidden="true">{running.length}</span>
          <span className="sr-only">{label}</span>
        </Dropdown.Trigger>
      </Tooltip>
      <Dropdown.Menu
        aria-label={LIST_LABEL}
        rounded=""
        className="overflow-hidden rounded-card border-line bg-main-900 p-0 shadow-pop"
      >
        <header className="flex items-center gap-2 border-b border-line px-3 py-2.5">
          <Icon path={mdiCogPlayOutline} size={15} className="flex-none text-accent-medium" />
          <span className="min-w-0 flex-1 text-[13px] font-semibold text-main-50">{label}</span>
        </header>
        <ScrollArea className="max-h-90">
          <ul className="m-0 list-none divide-y divide-line p-0">
            {store.shells.map((shell) => (
              <ShellRow key={shell.id} store={store} shell={shell} onOutput={onOutput} />
            ))}
          </ul>
        </ScrollArea>
      </Dropdown.Menu>
    </Dropdown>
  );
});
