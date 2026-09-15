import { observer } from "mobx-react-lite";
import { mdiChip, mdiExpansionCard, mdiMemory } from "@mdi/js";
import type { RuntimeDto, RuntimeState } from "@zvs/shared";
import Button from "../../ui/atoms/Button";
import Chip from "../../ui/atoms/Chip";
import Icon from "../../ui/atoms/Icon";
import StatusDot from "../../ui/atoms/StatusDot";
import { STATUS_TONE_TEXT, type StatusTone } from "../../ui/atoms/statusTone";
import type DownloadStore from "./DownloadStore";
import { formatBytes } from "./downloadPresentation";

const RUNTIME_STATE_LABELS: Record<RuntimeState, string> = {
  unsupported: "нет сборки",
  available: "доступно",
  installing: "устанавливается",
  installed: "установлено",
  starting: "запускается",
  ready: "работает",
  failed: "ошибка",
};

const RUNTIME_STATE_TONES: Record<RuntimeState, StatusTone> = {
  unsupported: "idle",
  available: "idle",
  installing: "accent",
  installed: "ok",
  starting: "accent",
  ready: "ok",
  failed: "err",
};

function RuntimeRow({
  runtime,
  busy,
  onInstall,
}: {
  readonly runtime: RuntimeDto;
  readonly busy: boolean;
  readonly onInstall: () => void;
}) {
  const tone = RUNTIME_STATE_TONES[runtime.state];
  const installable = runtime.state === "available" || runtime.state === "failed";
  return (
    <div className="flex items-start gap-3 border-t border-main-750 px-4 py-2.75 first:border-t-0">
      <span className="mt-0.5 flex size-7.5 flex-none items-center justify-center rounded-lg bg-main-700 text-main-300">
        <Icon path={runtime.accelerator === "cpu" ? mdiMemory : mdiExpansionCard} size={16} />
      </span>
      <div className="flex min-w-0 flex-1 flex-col gap-0.75">
        <div className="flex min-w-0 flex-wrap items-center gap-2">
          <span className="truncate text-[12.5px] font-medium text-main-100">
            {runtime.displayName}
          </span>
          {runtime.recommended ? <Chip tone="accent">рекомендуется</Chip> : null}
          <Chip>
            <span className="font-mono">{runtime.formats.join(" · ")}</span>
          </Chip>
          {runtime.installedVersion === undefined ? null : (
            <Chip>
              <span className="font-mono">{runtime.installedVersion}</span>
            </Chip>
          )}
        </div>
        <p className="m-0 text-[10.5px]/4  text-main-500">{runtime.description}</p>
        <span className="font-mono text-[10.5px] text-main-500">
          {formatBytes(runtime.sizeBytes)}
          {runtime.executablePath === undefined ? "" : ` · ${runtime.executablePath}`}
        </span>
        {runtime.blockedReason === undefined ? null : (
          <span className="text-[10.5px] text-main-500">{runtime.blockedReason}</span>
        )}
      </div>
      <Chip title={RUNTIME_STATE_LABELS[runtime.state]}>
        <StatusDot tone={tone} />
        <span className={STATUS_TONE_TEXT[tone]}>{RUNTIME_STATE_LABELS[runtime.state]}</span>
      </Chip>
      <Button
        tone={runtime.recommended ? "primary" : "secondary"}
        disabled={busy || !installable}
        title={runtime.blockedReason}
        onClick={onInstall}
      >
        {runtime.state === "failed" ? "Повторить" : "Установить"}
      </Button>
    </div>
  );
}

/**
 * The local stack, at the top of the catalogue: what this machine was detected to have, the
 * engine build that matches it, and one button that installs the whole working set.
 *
 * Embedding a document needs an engine *and* a weight file, and a user who has only ever used
 * an API provider has neither. Presenting them as one set — rather than two rows in a long
 * catalogue — is what makes the local path discoverable.
 */
function LocalStackPanel({ store }: { readonly store: DownloadStore }) {
  const overview = store.runtimes;
  if (overview === null) return null;
  const busy = store.runtimeBusyId !== null || store.busyId !== null;
  const gpu = overview.plan.gpu;
  const models = store.recommendedModels;
  const ready = store.localStackReady;

  return (
    <section className="border-b border-main-750">
      <header className="flex flex-wrap items-center gap-2.5 px-4 py-2.5 bg-main-900/40">
        <Icon path={mdiChip} size={15} className="flex-none text-accent-dark" />
        <span className="text-[11px] font-semibold tracking-[0.08em] text-main-400 uppercase">
          Локальные модели
        </span>
        <Chip>
          <StatusDot tone={ready ? "ok" : "idle"} />
          <span className={STATUS_TONE_TEXT[ready ? "ok" : "idle"]}>
            {ready ? "готово к индексации" : "не настроено"}
          </span>
        </Chip>
        <div className="flex-1" />
        <Button
          tone="primary"
          disabled={busy || ready}
          title="Скачать движок под эту машину и модель эмбеддингов к нему"
          onClick={() => {
            void store.installLocalStack();
          }}
        >
          Установить набор
        </Button>
      </header>

      <p className="m-0 px-4 pb-2.5 text-[11.5px]/4.5  text-main-400 bg-main-900/40">
        {overview.plan.reason}
        {gpu === null
          ? ""
          : ` Видеокарта: ${gpu.vendor} ${gpu.model}${
              gpu.vramBytes === null ? "" : ` · ${formatBytes(gpu.vramBytes)} видеопамяти`
            }.`}{" "}
        Модели в формате GGUF считаются этим движком.
      </p>

      {store.runtimeError === null ? null : (
        <p role="alert" className="m-0 px-4 pb-2.5 text-[11.5px] text-err">
          {store.runtimeError}
        </p>
      )}

      <div className="border-t border-main-750">
        {overview.runtimes
          .filter((runtime) => runtime.state !== "unsupported")
          .map((runtime) => (
            <RuntimeRow
              key={runtime.id}
              runtime={runtime}
              busy={busy}
              onInstall={() => {
                void store.installRuntime(runtime.id);
              }}
            />
          ))}
      </div>

      {models.length === 0 ? null : (
        <div className="flex flex-wrap items-center gap-2 border-t border-main-750 px-4 py-2.5">
          <span className="text-[11px] text-main-500">Модель эмбеддингов для набора:</span>
          {models.map((item) => (
            <Chip key={item.ref} tone={item.state === "installed" ? "accent" : "neutral"}>
              <StatusDot tone={item.state === "installed" ? "ok" : "idle"} />
              <span className="font-mono">{item.displayName}</span>
              {item.dimension === undefined ? null : <span>{item.dimension} dim</span>}
            </Chip>
          ))}
        </div>
      )}
    </section>
  );
}

export default observer(LocalStackPanel);
