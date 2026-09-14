import { mdiAccountCircleOutline, mdiFlashOutline, mdiKeyOutline } from "@mdi/js";
import { observer } from "mobx-react-lite";
import {
  PROVIDER_CAPABILITIES,
  PROVIDER_KINDS,
  type AccountId,
  type AdapterFamily,
  type ProviderKind,
  type SecretId,
  isAccountFamily,
} from "@zvs/shared";
import Button from "../../ui/atoms/Button";
import Chip from "../../ui/atoms/Chip";
import Icon from "../../ui/atoms/Icon";
import SelectInput from "../../ui/atoms/SelectInput";
import StatusDot from "../../ui/atoms/StatusDot";
import TextInput from "../../ui/atoms/TextInput";
import Field from "../../ui/molecules/Field";
import AuthModeSwitch from "./AuthModeSwitch";
import ProbeResultLine from "./ProbeResultLine";
import type { ProviderFormVm } from "./ProviderFormVm";
import {
  ADAPTER_LABELS,
  CAPABILITY_LABELS,
  KIND_LABELS,
  secretsForKind,
  STATUS_LABELS,
  statusTone,
} from "./providerPresentation";
import { useStore } from "../../stores/useStore";

interface ConnectionCardProps {
  readonly vm: ProviderFormVm;
  readonly onManageSecrets: () => void;
}

function ConnectionCard({ vm, onManageSecrets }: ConnectionCardProps) {
  const { providers } = useStore();
  const summary = providers.selectedSummary;
  const pickable = secretsForKind(providers.secrets, vm.kind);
  const account = vm.account;
  const hasAuthModeChoice = !vm.isLocal && vm.descriptor.authModes.length > 1;
  const showsVendor = !vm.isLocal && !isAccountFamily(vm.adapter);

  return (
    <section className="flex min-w-0 flex-[1.35] flex-col gap-3.25 rounded-card border border-main-750 bg-main-900 p-4">
      <div className="flex items-center gap-2">
        <h2 className="flex-1 text-[11px] font-semibold tracking-[0.08em] text-main-400 uppercase">
          Настройка подключения
        </h2>
        {summary === null ? null : (
          <Chip>
            <StatusDot tone={statusTone(summary.status)} />
            {STATUS_LABELS[summary.status]}
          </Chip>
        )}
      </div>

      <div className="grid grid-cols-2 gap-3.25">
        <Field
          label="Семейство адаптера"
          help={
            vm.descriptor.implemented ? undefined : "Семейство объявлено, но ещё не реализован."
          }
          error={vm.errorOf("adapter")}
        >
          <SelectInput
            label="Семейство адаптера"
            value={vm.adapter}
            options={vm.adapterOptions.map((entry) => ({
              value: entry.family,
              label: entry.implemented
                ? ADAPTER_LABELS[entry.family]
                : `${ADAPTER_LABELS[entry.family]} — не реализован`,
            }))}
            onChange={(value) => {
              vm.setAdapter(value as AdapterFamily);
            }}
          />
        </Field>
        <Field
          label="Название подключения"
          htmlFor="provider-name"
          required
          error={vm.errorOf("name")}
        >
          <TextInput
            id="provider-name"
            value={vm.name}
            maxLength={128}
            placeholder="Ollama"
            invalid={vm.errorOf("name") !== undefined}
            onChange={(event) => {
              vm.setName(event.target.value);
            }}
          />
        </Field>
      </div>

      {showsVendor || hasAuthModeChoice ? (
        <div
          className={`grid gap-3.25 ${showsVendor && hasAuthModeChoice ? "grid-cols-2" : "grid-cols-1"}`}
        >
          {showsVendor ? (
            <Field label="Вендор">
              <SelectInput
                label="Вендор"
                value={vm.kind}
                options={PROVIDER_KINDS.map((kind) => ({ value: kind, label: KIND_LABELS[kind] }))}
                onChange={(value) => {
                  vm.setKind(value as ProviderKind);
                }}
              />
            </Field>
          ) : null}
          {hasAuthModeChoice ? (
            <Field label="Способ авторизации" error={vm.errorOf("authMode")}>
              <AuthModeSwitch
                value={vm.authMode}
                disabledReason={vm.authModeDisabledReason}
                onChange={vm.setAuthMode}
              />
            </Field>
          ) : null}
        </div>
      ) : null}

      {vm.isLocal ? (
        <p className="rounded-[6px] border border-dashed border-main-750 px-3 py-2.25 text-[12px] text-main-400">
          Модели читаются из папки загрузок этого компьютера. Ничего настраивать не нужно — нажмите
          «Найти модели», чтобы увидеть найденные файлы.
        </p>
      ) : vm.usesAccount ? (
        <Field label="Связанный аккаунт" error={vm.errorOf("accountId")}>
          {vm.accountOptions.length === 0 ? (
            <div className="flex items-center gap-2.5 rounded-[6px] border border-dashed border-main-750 px-2.5 py-2 text-[12px] text-main-400">
              <Icon path={mdiAccountCircleOutline} size={16} className="flex-none text-main-500" />
              <span className="flex-1">
                Нет привязанных аккаунтов для {ADAPTER_LABELS[vm.adapter]}.
              </span>
            </div>
          ) : (
            <div className="flex gap-2">
              <div className="flex min-w-0 flex-1 items-center gap-2.5 rounded-[6px] bg-main-900 px-2.5 py-1.5">
                <Icon
                  path={mdiAccountCircleOutline}
                  size={16}
                  className="flex-none text-accent-dark"
                />
                <span className="min-w-0 flex-1">
                  <SelectInput
                    label="Связанный аккаунт"
                    value={vm.accountId ?? ""}
                    options={vm.accountOptions.map((entry) => ({
                      value: entry.id,
                      label: entry.emailMasked ?? entry.displayName ?? entry.id,
                    }))}
                    onChange={(value) => {
                      vm.setAccountId(value === "" ? null : (value as AccountId));
                    }}
                  />
                </span>
                {account === null ? null : (
                  <Chip tone={account.status === "linked" ? "accent" : "neutral"}>
                    {account.status === "linked"
                      ? "сессия активна"
                      : (account.detail ?? "нужна повторная привязка")}
                  </Chip>
                )}
              </div>
            </div>
          )}
        </Field>
      ) : (
        <>
          <Field
            label="Базовый адрес"
            htmlFor="provider-base-url"
            required
            error={vm.errorOf("baseUrl")}
          >
            <TextInput
              id="provider-base-url"
              mono
              value={vm.baseUrl}
              placeholder="https://ollama.com/v1"
              invalid={vm.errorOf("baseUrl") !== undefined}
              onChange={(event) => {
                vm.setBaseUrl(event.target.value);
              }}
            />
          </Field>

          <Field
            label="API-токен"
            help="Значение берётся из вкладки `Секреты`."
            error={vm.errorOf("secretId")}
          >
            <div className="flex gap-2">
              <span className="min-w-0 flex-1">
                <SelectInput
                  label="API-токен"
                  value={vm.secretId ?? ""}
                  placeholder="Выберите секрет"
                  invalid={vm.errorOf("secretId") !== undefined}
                  options={pickable.map((secret) => ({
                    value: secret.id,
                    label: `${secret.name} — ${secret.hint}`,
                  }))}
                  onChange={(value) => {
                    vm.setSecretId(value === "" ? null : (value as SecretId));
                  }}
                />
              </span>
              <Button type="button" tone="ghost" onClick={onManageSecrets}>
                <Icon path={mdiKeyOutline} size={15} />
                Секреты
              </Button>
            </div>
          </Field>
        </>
      )}

      {vm.isLocal ? null : (
        <Field label="Возможности" error={vm.errorOf("capabilities")}>
          <div className="flex flex-wrap gap-1.5">
            {PROVIDER_CAPABILITIES.map((capability) => {
              const reason = vm.capabilityDisabledReason(capability);
              const active = vm.capabilities.includes(capability);
              return (
                <Chip
                  key={capability}
                  tone={active ? "selected" : "neutral"}
                  title={reason ?? undefined}
                  onClick={
                    reason === null
                      ? () => {
                          vm.toggleCapability(capability);
                        }
                      : undefined
                  }
                >
                  {CAPABILITY_LABELS[capability]}
                </Chip>
              );
            })}
          </div>
        </Field>
      )}

      <div className="mt-0.5 flex items-center gap-3">
        <Button
          type="button"
          tone="primary"
          disabled={providers.probing}
          onClick={() => {
            void providers.probe();
          }}
        >
          <Icon path={mdiFlashOutline} size={15} />
          {providers.probing
            ? vm.isLocal
              ? "Ищем…"
              : "Проверяем…"
            : vm.isLocal
              ? "Найти модели"
              : "Проверить связь"}
        </Button>
        <ProbeResultLine result={vm.probeResult} probing={providers.probing} />
      </div>

      {vm.banner === null ? null : (
        <p
          role="alert"
          className="rounded-[6px] border border-err-border bg-main-800 px-3 py-2.25 text-[12px] text-err"
        >
          {vm.banner}
        </p>
      )}
    </section>
  );
}

export default observer(ConnectionCard);
