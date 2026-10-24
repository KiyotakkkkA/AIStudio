import { mdiArrowRight, mdiMessageTextOutline, mdiTuneVariant } from "@mdi/js";
import { Loader } from "@kiyotakkkka/zvs-uikit-lib";
import { observer } from "mobx-react-lite";
import { Link } from "react-router-dom";
import Icon from "../../ui/atoms/Icon";
import VendorMark from "../providers/VendorMark";
import { adapterLabel } from "../providers/accountPresentation";
import type ChatStore from "./ChatStore";

export default observer(function ChatModelGate({ store }: { readonly store: ChatStore }) {
  const choices = store.accountChoices;

  return (
    <section className="rounded-card border border-line bg-main-900 p-4 shadow-card">
      <div className="flex min-w-0 items-start gap-3">
        <span className="flex size-9 flex-none items-center justify-center rounded-control bg-accent-soft text-accent-medium">
          <Icon path={mdiMessageTextOutline} size={18} />
        </span>
        <div className="flex min-w-0 flex-1 flex-col gap-1">
          <p className="text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase">
            Модель не выбрана
          </p>
          <p className="text-[13.5px] font-semibold text-main-50">
            Выберите доступную текстовую модель, чтобы начать диалог
          </p>
          <p className="text-[12px] text-main-400">
            {choices.length > 0
              ? "Подключите привязанный аккаунт или настройте провайдера ИИ."
              : "Добавьте провайдера ИИ с текстовой моделью или привяжите аккаунт вендора."}
          </p>
        </div>
        <Link
          to="/providers"
          className="zvs-press inline-flex h-8 flex-none items-center gap-1.5 rounded-control border border-line bg-main-900 px-3 text-[13px] font-medium text-main-100 shadow-[0_1px_2px_rgb(40_25_10/0.05)] hover:border-line-strong hover:bg-main-850"
        >
          <Icon path={mdiTuneVariant} size={15} />
          Провайдеры ИИ
        </Link>
      </div>

      {choices.length > 0 ? (
        <div className="mt-4 grid gap-2 sm:grid-cols-2">
          {choices.map((choice) => {
            const connecting = store.connectingAccountId === choice.accountId;
            return (
              <button
                key={choice.value}
                type="button"
                aria-label={`Подключить ${adapterLabel(choice.adapter)}`}
                disabled={store.connecting}
                onClick={() => void store.connectAccount(choice.accountId)}
                className="zvs-press group flex min-w-0 items-center gap-3 rounded-control border border-line bg-main-850 px-3 py-2.5 text-left hover:border-accent-line hover:bg-main-800 disabled:cursor-not-allowed disabled:opacity-60 disabled:hover:border-line disabled:hover:bg-main-850"
              >
                <VendorMark family={choice.adapter} size="md" />
                <span className="flex min-w-0 flex-1 flex-col gap-0.5">
                  <span className="text-[13px] font-medium text-main-50">
                    {connecting ? "Подключение…" : `Подключить ${adapterLabel(choice.adapter)}`}
                  </span>
                  <span className="truncate text-[11.5px] text-main-400">
                    {choice.detail ?? "Привязанный аккаунт"}
                  </span>
                </span>
                {connecting ? (
                  <Loader className="size-4 flex-none text-accent-dark" />
                ) : (
                  <Icon
                    path={mdiArrowRight}
                    size={16}
                    className="flex-none text-main-500 transition-transform group-hover:translate-x-0.5 group-hover:text-accent-medium group-disabled:translate-x-0 group-disabled:text-main-500"
                  />
                )}
              </button>
            );
          })}
        </div>
      ) : null}
    </section>
  );
});
