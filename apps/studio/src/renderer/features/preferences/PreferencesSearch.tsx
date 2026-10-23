import { observer } from "mobx-react-lite";
import { useId, useState } from "react";
import { mdiMagnify } from "@mdi/js";
import Icon from "../../ui/atoms/Icon";
import { useStore } from "../../stores/useStore";
import {
  SEARCH_EMPTY,
  SEARCH_LABEL,
  SEARCH_PLACEHOLDER,
  SECTION_LABELS,
} from "./preferencesPresentation";
import { searchSettings, type SettingMatch } from "./settingsIndex";

function Highlighted({ match }: { readonly match: SettingMatch }) {
  const { label } = match.entry;
  return (
    <span className="min-w-0 flex-1 truncate">
      {label.slice(0, match.start)}
      <mark className="rounded-[3px] bg-accent-soft px-px text-accent-medium">
        {label.slice(match.start, match.end)}
      </mark>
      {label.slice(match.end)}
    </span>
  );
}

function PreferencesSearch() {
  const { ui } = useStore();
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const listId = useId();
  const matches = searchSettings(query);
  const open = query.trim().length > 0;

  const choose = (match: SettingMatch | undefined) => {
    if (match === undefined) return;
    ui.revealPreference(match.entry.section, match.entry.id);
    setQuery("");
    setActive(0);
  };

  return (
    <div className="relative px-0.5 pb-2.5">
      <label className="flex h-8 items-center gap-2 rounded-control bg-main-850 px-2.5 text-main-500 focus-within:text-main-300">
        <Icon path={mdiMagnify} size={15} className="flex-none" />
        <input
          role="combobox"
          aria-label={SEARCH_LABEL}
          aria-expanded={open}
          aria-controls={listId}
          aria-autocomplete="list"
          placeholder={SEARCH_PLACEHOLDER}
          value={query}
          onChange={(event) => {
            setQuery(event.target.value);
            setActive(0);
          }}
          onKeyDown={(event) => {
            if (!open) return;
            if (event.key === "ArrowDown") {
              event.preventDefault();
              setActive((index) => Math.min(index + 1, matches.length - 1));
            } else if (event.key === "ArrowUp") {
              event.preventDefault();
              setActive((index) => Math.max(index - 1, 0));
            } else if (event.key === "Enter") {
              event.preventDefault();
              choose(matches[active]);
            } else if (event.key === "Escape") {
              event.preventDefault();
              event.stopPropagation();
              setQuery("");
            }
          }}
          className="min-w-0 flex-1 bg-transparent text-[12.5px] text-main-100 outline-none placeholder:text-main-500"
        />
      </label>
      {open ? (
        <div
          id={listId}
          role="listbox"
          aria-label={SEARCH_LABEL}
          className="absolute top-full left-0 z-20 -mt-1 flex w-80 flex-col gap-0.5 rounded-card border border-line bg-main-900 p-1.5 shadow-pop"
        >
          {matches.length === 0 ? (
            <p className="m-0 px-2 py-3 text-center text-[12px] text-main-500">{SEARCH_EMPTY}</p>
          ) : (
            matches.map((match, index) => (
              <button
                key={match.entry.id}
                type="button"
                role="option"
                aria-selected={index === active}
                onMouseEnter={() => setActive(index)}
                onClick={() => choose(match)}
                className={`flex h-8 items-center gap-3 rounded-control px-2 text-left text-[12.5px] text-main-100 ${
                  index === active ? "bg-main-800" : ""
                }`}
              >
                <Highlighted match={match} />
                <span className="flex-none text-[11.5px] text-main-500">
                  {SECTION_LABELS[match.entry.section]}
                </span>
              </button>
            ))
          )}
        </div>
      ) : null}
    </div>
  );
}

export default observer(PreferencesSearch);
