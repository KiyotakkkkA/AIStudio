import { observer } from "mobx-react-lite";
import type { ChatEventDto } from "@zvs/shared";
import StatusDot from "../../ui/atoms/StatusDot";
import { useStore } from "../../stores/useStore";
import {
  OUTPUT,
  PROCESS_ENDED,
  WOKEN_TURN,
  commandText,
  endedTone,
  exitCodeText,
} from "./backgroundShellsCopy";

export default observer(function ProcessEventCard({ event }: { readonly event: ChatEventDto }) {
  const { chat } = useStore();
  return (
    <section className="rounded-card bg-main-850 px-3.5 py-2.5" aria-label={PROCESS_ENDED}>
      <p className="m-0 mb-1.5 flex flex-wrap items-baseline gap-x-2 text-[12px]">
        <span className="font-medium text-main-100">{PROCESS_ENDED}</span>
        <span className="text-main-400">{WOKEN_TURN}</span>
      </p>
      <ul className="m-0 list-none space-y-1 p-0">
        {event.shells.map((ended) => (
          <li key={ended.shell.id} className="flex min-w-0 items-center gap-2 text-[11.5px]">
            <StatusDot tone={endedTone(ended.shell)} title={exitCodeText(ended.shell.exitCode)} />
            <span
              className="min-w-0 flex-1 truncate font-mono text-main-200"
              title={commandText(ended.shell)}
            >
              {commandText(ended.shell)}
            </span>
            <span className="shrink-0 font-mono text-main-400 tabular-nums">
              {exitCodeText(ended.shell.exitCode)}
            </span>
            <button
              type="button"
              onClick={() => chat.openShellOutput(ended.shell.id)}
              className="shrink-0 rounded-control px-2 py-0.5 text-[12px] text-accent-medium hover:bg-main-750"
            >
              {OUTPUT}
            </button>
          </li>
        ))}
      </ul>
    </section>
  );
});
