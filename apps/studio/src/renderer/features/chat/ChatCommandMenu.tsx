import { mdiConsoleLine } from "@mdi/js";
import { observer } from "mobx-react-lite";
import Icon from "../../ui/atoms/Icon";
import PluginMark from "../plugins/PluginMark";
import type ChatStore from "./ChatStore";
import { ACCOUNTS_LABEL, COMMANDS_LABEL, type MenuEntry } from "./chatCommands";

export const COMMAND_MENU_ID = "chat-command-menu";

export function commandOptionId(index: number): string {
  return `${COMMAND_MENU_ID}-${index}`;
}

function EntryMark({ entry }: { readonly entry: MenuEntry }) {
  if (entry.type === "command") {
    return (
      <span
        aria-hidden="true"
        className="flex size-4.5 flex-none items-center justify-center text-main-400"
      >
        <Icon path={mdiConsoleLine} size={14} />
      </span>
    );
  }
  return (
    <PluginMark kind={entry.type === "plugin" ? entry.plugin : entry.account.kind} size="xs" />
  );
}

export default observer(function ChatCommandMenu({ store }: { readonly store: ChatStore }) {
  const matches = store.commandMatches;
  if (matches.length === 0) return null;
  return (
    <ul
      id={COMMAND_MENU_ID}
      role="listbox"
      aria-label={store.pickingPlugin === null ? COMMANDS_LABEL : ACCOUNTS_LABEL}
      className="absolute inset-x-2.5 bottom-full z-20 m-0 mb-2 list-none rounded-card border border-line bg-main-900 p-1 shadow-pop"
    >
      {matches.map((entry, index) => {
        const selected = index === store.commandIndex;
        return (
          <li
            key={`${entry.type}:${entry.key}`}
            id={commandOptionId(index)}
            role="option"
            aria-selected={selected}
            className={`flex cursor-pointer items-center gap-2.5 rounded-lg px-2.5 py-2 ${
              selected ? "bg-main-700" : "hover:bg-main-800"
            }`}
            onMouseDown={(event) => event.preventDefault()}
            onMouseEnter={() => store.moveCommand(index - store.commandIndex)}
            onClick={() => store.completeCommand(entry)}
          >
            <EntryMark entry={entry} />
            <span
              className={`shrink-0 text-[12.5px] font-medium text-main-50 ${
                entry.type === "account" ? "" : "font-mono"
              }`}
            >
              {entry.label}
            </span>
            <span className="min-w-0 truncate text-[12px] text-main-400">{entry.description}</span>
          </li>
        );
      })}
    </ul>
  );
});
