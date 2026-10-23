import { mdiMagnify, mdiPin, mdiPinOutline } from "@mdi/js";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import { observer } from "mobx-react-lite";
import IconButton from "../../ui/atoms/buttons/IconButton";
import Icon from "../../ui/atoms/Icon";
import TextInput from "../../ui/atoms/TextInput";
import { iconFor } from "./flowModel";
import { descriptionOf } from "./nodeForms";
import type ScenarioEditor from "./ScenarioEditor";
import { ROLE_LABELS } from "./scenarioCopy";

export interface NodePaletteProps {
  readonly editor: ScenarioEditor;
  readonly onPick: (type: string) => void;
}

function NodePalette({ editor, onPick }: NodePaletteProps) {
  const groups = editor.palette;
  return (
    <div
      role="dialog"
      aria-label="Добавление шага"
      className="absolute top-15 right-4.5 z-20 flex max-h-[calc(100%-96px)] w-75 flex-col overflow-hidden rounded-card border border-line bg-main-900 shadow-pop"
    >
      <div className="flex flex-none items-center gap-1.5 border-b border-line p-2.5">
        <div className="min-w-0 flex-1">
          <TextInput
            preset="search"
            aria-label="Поиск шага"
            placeholder="Поиск шага"
            value={editor.paletteQuery}
            autoFocus
            onChange={(event) => {
              editor.setPaletteQuery(event.target.value);
            }}
            onKeyDown={(event) => {
              if (event.key === "Escape") editor.closePalette();
            }}
          />
        </div>
        <IconButton
          path={editor.palettePinned ? mdiPin : mdiPinOutline}
          label={editor.palettePinned ? "Открепить список" : "Закрепить список"}
          tooltip="bottom-right"
          iconSize={16}
          aria-pressed={editor.palettePinned}
          onClick={editor.togglePalettePin}
          className={`size-8 ${editor.palettePinned ? "bg-accent-soft text-accent-medium hover:bg-accent-soft hover:text-accent-medium" : ""}`}
        />
      </div>
      <ScrollArea className="min-h-0 flex-1 p-1.5">
        {groups.length === 0 ? (
          <div className="flex items-center gap-2 px-2.5 py-3 text-[12px] text-main-500">
            <Icon path={mdiMagnify} size={15} />
            Ничего не найдено
          </div>
        ) : (
          groups.map((group) => (
            <section key={group.key} className="mb-1.5 last:mb-0">
              <h3 className="m-0 px-2.5 pt-2 pb-1 text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase">
                {group.label}
              </h3>
              {group.entries.map((entry) => (
                <button
                  key={entry.type}
                  type="button"
                  onClick={() => {
                    onPick(entry.type);
                  }}
                  className="flex w-full items-center gap-2.5 rounded-control px-2.5 py-1.75 text-left hover:bg-main-750"
                >
                  <span className="flex size-7 flex-none items-center justify-center rounded-[7px] bg-main-700 text-accent-medium">
                    <Icon path={iconFor(entry.type, entry.spec.toolKey)} size={15} />
                  </span>
                  <span className="flex min-w-0 flex-1 flex-col">
                    <span className="truncate text-[12.5px] font-medium text-main-100">
                      {entry.label}
                    </span>
                    <span
                      className="truncate text-[10.5px] text-main-500"
                      title={descriptionOf(entry.type) ?? entry.type}
                    >
                      {descriptionOf(entry.type) ??
                        `${ROLE_LABELS[entry.spec.role]} · ${entry.type}`}
                    </span>
                  </span>
                </button>
              ))}
            </section>
          ))
        )}
      </ScrollArea>
    </div>
  );
}

export default observer(NodePalette);
