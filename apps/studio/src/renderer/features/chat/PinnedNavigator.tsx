import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import { PINS_NAVIGATION } from "./pinnedMessages";

export default function PinnedNavigator({
  items,
  current,
  onSelect,
}: {
  readonly items: readonly { readonly id: string; readonly label: string }[];
  readonly current: string | null;
  readonly onSelect: (id: string) => void;
}) {
  return (
    <nav
      aria-label={PINS_NAVIGATION}
      className="group/pins absolute top-1/2 right-3 z-10 flex max-h-[70%] -translate-y-1/2 flex-col"
    >
      <ScrollArea className="min-h-0 rounded-card border border-transparent p-1 transition-colors group-focus-within/pins:border-line group-focus-within/pins:bg-main-850 group-focus-within/pins:shadow-pop group-hover/pins:border-line group-hover/pins:bg-main-850 group-hover/pins:shadow-pop">
        <ul className="m-0 flex list-none flex-col p-0">
          {items.map((item) => {
            const active = item.id === current;
            return (
              <li key={item.id}>
                <button
                  type="button"
                  title={item.label}
                  aria-current={active ? "location" : undefined}
                  onClick={() => onSelect(item.id)}
                  className={`flex h-6.5 w-full cursor-pointer items-center justify-end gap-3 rounded-control pr-1 pl-0 text-left outline-none group-focus-within/pins:pl-2.5 group-hover/pins:pl-2.5 hover:bg-main-800 focus-visible:bg-main-800 ${
                    active ? "text-main-50" : "text-main-400 hover:text-main-100"
                  }`}
                >
                  <span
                    className={`hidden w-52 min-w-0 truncate text-[12px] group-focus-within/pins:block group-hover/pins:block ${active ? "font-medium" : ""}`}
                  >
                    {item.label}
                  </span>
                  <span
                    aria-hidden
                    className={`h-0.5 flex-none rounded-full ${active ? "w-3.5 bg-main-50" : "w-2.5 bg-main-500"}`}
                  />
                </button>
              </li>
            );
          })}
        </ul>
      </ScrollArea>
    </nav>
  );
}
