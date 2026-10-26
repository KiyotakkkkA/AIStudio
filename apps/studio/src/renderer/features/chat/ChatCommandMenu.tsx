import { observer } from "mobx-react-lite";
import type ChatStore from "./ChatStore";
import { COMMANDS_LABEL, commandText } from "./chatCommands";

export const COMMAND_MENU_ID = "chat-command-menu";

export function commandOptionId(index: number): string {
  return `${COMMAND_MENU_ID}-${index}`;
}

export default observer(function ChatCommandMenu({ store }: { readonly store: ChatStore }) {
  const matches = store.commandMatches;
  if (matches.length === 0) return null;
  return (
    <ul
      id={COMMAND_MENU_ID}
      role="listbox"
      aria-label={COMMANDS_LABEL}
      className="absolute inset-x-2.5 bottom-full z-20 m-0 mb-2 list-none rounded-card border border-line bg-main-900 p-1 shadow-pop"
    >
      {matches.map((item, index) => {
        const selected = index === store.commandIndex;
        return (
          <li
            key={item.command}
            id={commandOptionId(index)}
            role="option"
            aria-selected={selected}
            className={`flex cursor-pointer items-baseline gap-3 rounded-lg px-2.5 py-2 ${
              selected ? "bg-main-700" : "hover:bg-main-800"
            }`}
            onMouseDown={(event) => event.preventDefault()}
            onMouseEnter={() => store.moveCommand(index - store.commandIndex)}
            onClick={() => store.completeCommand(item.command)}
          >
            <span className="shrink-0 font-mono text-[12.5px] font-medium text-main-50">
              {commandText(item.command)}
            </span>
            <span className="min-w-0 truncate text-[12px] text-main-400">{item.description}</span>
          </li>
        );
      })}
    </ul>
  );
});
