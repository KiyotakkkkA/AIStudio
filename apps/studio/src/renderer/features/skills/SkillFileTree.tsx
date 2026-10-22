import {
  mdiFileDocumentOutline,
  mdiFileOutline,
  mdiFolderOutline,
  mdiFolderOpenOutline,
  mdiPlus,
} from "@mdi/js";
import { observer } from "mobx-react-lite";
import { ScrollArea, TreeView } from "@kiyotakkkka/zvs-uikit-lib";
import { useRef, useState } from "react";
import Icon from "../../ui/atoms/Icon";
import type { SkillFormVm } from "./SkillFormVm";
import { MANIFEST_PATH } from "./SkillStore";
import type SkillStore from "./SkillStore";
import { readFiles } from "./readFiles";
import { FILE_DIRS, formatSize, type SkillFileDir } from "./skillPresentation";

const ELEMENT = {
  content: "min-w-0",
  label: "truncate font-mono text-[12px]",
  rightSlot: "font-mono text-[10px] text-main-500",
};

function SkillFileTree({ store, vm }: { readonly store: SkillStore; readonly vm: SkillFormVm }) {
  const input = useRef<HTMLInputElement | null>(null);
  const [target, setTarget] = useState<SkillFileDir>("references");
  const rows = vm.fileRows;

  const pick = (dir: SkillFileDir) => {
    setTarget(dir);
    input.current?.click();
  };

  return (
    <div className="flex w-60 flex-none flex-col border-r border-line">
      <input
        ref={input}
        type="file"
        multiple
        hidden
        onChange={(event) => {
          const files = event.target.files;
          if (files === null || files.length === 0) return;
          void readFiles(files, target).then((read) => {
            store.addFiles(read);
            const [first] = read;
            if (first !== undefined) void store.open(first.path);
          });
          event.target.value = "";
        }}
      />
      <div className="flex h-9 flex-none items-center px-3 text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase">
        Файлы навыка
      </div>
      <ScrollArea className="flex min-h-0 flex-1 flex-col px-1.5 pb-3">
        <TreeView aria-label="Файлы навыка">
          <TreeView.Element
            label={MANIFEST_PATH}
            description="Описание и инструкции"
            selected={store.openPath === MANIFEST_PATH}
            icon={<Icon path={mdiFileDocumentOutline} size={15} />}
            classNames={ELEMENT}
            onClick={() => void store.open(MANIFEST_PATH)}
          />
          {FILE_DIRS.map((dir) => {
            const inDir = rows.filter((row) => row.path.startsWith(`${dir}/`));
            return (
              <TreeView.Catalog
                key={dir}
                title={<span className="font-mono text-[12px]">{dir}/</span>}
                defaultOpen={inDir.length > 0}
                icon={<Icon path={mdiFolderOutline} size={15} />}
                openIcon={<Icon path={mdiFolderOpenOutline} size={15} />}
                rightSlot={
                  <span
                    role="button"
                    tabIndex={0}
                    aria-label={`Добавить файл в ${dir}/`}
                    title="Добавить файл"
                    className="flex size-5 items-center justify-center rounded-control text-main-400 hover:bg-main-700 hover:text-main-50"
                    onClick={(event) => {
                      event.stopPropagation();
                      pick(dir);
                    }}
                    onKeyDown={(event) => {
                      if (event.key !== "Enter") return;
                      event.stopPropagation();
                      pick(dir);
                    }}
                  >
                    <Icon path={mdiPlus} size={13} />
                  </span>
                }
              >
                {inDir.length === 0 ? (
                  <span className="block px-2 py-1 text-[11.5px] text-main-500">Пусто</span>
                ) : null}
                {inDir.map((row) => (
                  <TreeView.Element
                    key={row.path}
                    label={row.path.slice(dir.length + 1)}
                    selected={store.openPath === row.path}
                    icon={<Icon path={mdiFileOutline} size={14} />}
                    rightSlot={row.added ? "новый" : formatSize(row.size)}
                    classNames={{
                      ...ELEMENT,
                      label: `${ELEMENT.label} ${vm.fileErrorOf(row.path) === undefined ? "" : "text-err"}`,
                    }}
                    onClick={() => void store.open(row.path)}
                  />
                ))}
              </TreeView.Catalog>
            );
          })}
        </TreeView>
      </ScrollArea>
      {vm.removed.length + vm.added.length > 0 ? (
        <p className="flex-none border-t border-line px-3 py-2 text-[11px] text-warn">
          Изменения файлов применятся после сохранения.
        </p>
      ) : null}
    </div>
  );
}

export default observer(SkillFileTree);
