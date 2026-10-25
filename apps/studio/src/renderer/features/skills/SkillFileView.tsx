import { mdiDeleteOutline, mdiFileOutline } from "@mdi/js";
import { observer } from "mobx-react-lite";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import { CodeView } from "@kiyotakkkka/zvs-uikit-lib/code-view";
import Button from "../../ui/atoms/buttons/Button";
import Icon from "../../ui/atoms/Icon";
import EmptyState from "../../ui/molecules/EmptyState";
import type { SkillFormVm } from "./SkillFormVm";
import type SkillStore from "./SkillStore";
import { formatSize } from "./skillPresentation";
import Alert from "../../ui/molecules/Alert";

const LANGUAGES: Record<string, string> = {
  py: "python",
  js: "javascript",
  mjs: "javascript",
  cjs: "javascript",
  ts: "typescript",
  json: "json",
  md: "markdown",
  yaml: "yaml",
  yml: "yaml",
  toml: "toml",
  html: "html",
  css: "css",
  csv: "csv",
  sh: "bash",
  xml: "xml",
};

function languageOf(path: string): string {
  const extension = path.split(".").pop()?.toLowerCase() ?? "";
  return LANGUAGES[extension] ?? "text";
}

function SkillFileView({ store, vm }: { readonly store: SkillStore; readonly vm: SkillFormVm }) {
  const path = store.openPath;
  const row = vm.fileRows.find((entry) => entry.path === path) ?? null;
  const file = store.openFile;
  const error = vm.fileErrorOf(path);

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col">
      <div className="flex h-11 flex-none items-center gap-2.5 border-b border-line px-4.5">
        <Icon path={mdiFileOutline} size={15} />
        <span className="min-w-0 flex-1 truncate font-mono text-[12.5px] text-main-100">
          {path}
        </span>
        {row === null ? null : (
          <span className="font-mono text-[11px] text-main-500">{formatSize(row.size)}</span>
        )}
        <Button tone="ghost" onClick={store.removeOpenFile}>
          <Icon path={mdiDeleteOutline} size={15} />
          Удалить файл
        </Button>
      </div>
      {error === undefined ? null : <Alert className="mx-4.5 mt-3">{error}</Alert>}
      <div className="flex min-h-0 flex-1 flex-col p-4.5">
        {row?.added === true ? (
          <div className="flex flex-1 items-center justify-center">
            <EmptyState
              icon={mdiFileOutline}
              title="Новый файл"
              description="Файл будет добавлен в навык после сохранения."
            />
          </div>
        ) : store.fileLoading || file === null ? (
          <span className="text-[12px] text-main-500">Загрузка файла…</span>
        ) : file.text === null ? (
          <div className="flex flex-1 items-center justify-center">
            <EmptyState
              icon={mdiFileOutline}
              title="Двоичный файл"
              description={`Содержимое не отображается · ${formatSize(file.size)}`}
            />
          </div>
        ) : (
          <ScrollArea className="min-h-0 flex-1 rounded-card">
            <CodeView
              code={file.text}
              language={languageOf(path)}
              fileName={path.split("/").pop()}
              downloadable={false}
            />
          </ScrollArea>
        )}
      </div>
    </div>
  );
}

export default observer(SkillFileView);
