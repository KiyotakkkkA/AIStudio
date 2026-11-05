import { mdiAccountCircleOutline, mdiKeyOutline } from "@mdi/js";
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
import Button from "../../ui/atoms/buttons/Button";
import Chip from "../../ui/atoms/Chip";
import Icon from "../../ui/atoms/Icon";
import SelectInput from "../../ui/atoms/SelectInput";
import StatusDot from "../../ui/atoms/StatusDot";
import TextInput from "../../ui/atoms/TextInput";
import Field from "../../ui/molecules/Field";
import AuthModeSwitch from "./AuthModeSwitch";
import type { ProviderFormVm } from "./ProviderFormVm";
import {
  ADAPTER_LABELS,
  CAPABILITY_LABELS,
  KIND_LABELS,
  PROVIDER_TYPE_LABEL,
  STATUS_LABELS,
  statusTone,
} from "./providerPresentation";
import { useStore } from "../../stores/useStore";
import Alert from "../../ui/molecules/Alert";

interface ConnectionCardProps {
  readonly vm: ProviderFormVm;
  readonly onManageSecrets: () => void;
}

function ConnectionCard({ vm, onManageSecrets }: ConnectionCardProps) {
  const { providers } = useStore();
  const summary = providers.selectedSummary;
  const pickable = providers.secrets;
  const account = vm.account;
  const hasAuthModeChoice = !vm.isLocal && vm.descriptor.authModes.length > 1;
  const showsVendor = !vm.isLocal && !isAccountFamily(vm.adapter);

  return (
    <section className="flex min-w-0 flex-col gap-3.5 rounded-xl bg-main-850 p-5">
      <div className="flex items-center gap-2">
        <h2 className="flex-1 text-[15px] font-semibold tracking-[-0.01em] text-main-100">
          Настройка подключения
        </h2>
        {summary === null ? null : (
          <span
            className={`inline-flex items-center gap-1.5 rounded-full px-2 py-1 text-[11px] font-medium ${summary.status === "ok" ? "bg-ok-soft text-ok" : summary.status === "failed" ? "bg-err-soft text-err" : summary.status === "unknown" ? "bg-main-700 text-main-300" : "bg-warn-soft text-warn"}`}
          >
            <StatusDot tone={statusTone(summary.status)} />
            {STATUS_LABELS[summary.status]}
          </span>
        )}
      </div>

      <div className="flex flex-col gap-3">
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
          <Field
            label={PROVIDER_TYPE_LABEL}
            help={
              vm.descriptor.implemented ? undefined : "Семейство объявлено, но ещё не реализован."
            }
            error={vm.errorOf("adapter")}
          >
            <SelectInput
              label={PROVIDER_TYPE_LABEL}
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
      </div>

      {vm.isLocal ? null : (
        <div className="flex flex-col gap-3">
          {hasAuthModeChoice ? (
            <Field label="Способ авторизации" error={vm.errorOf("authMode")}>
              <AuthModeSwitch
                value={vm.authMode}
                disabledReason={vm.authModeDisabledReason}
                onChange={vm.setAuthMode}
              />
            </Field>
          ) : null}
          {vm.usesAccount ? (
            <Field label="Связанный аккаунт" error={vm.errorOf("accountId")}>
              {vm.accountOptions.length === 0 ? (
                <div className="flex items-center gap-2.5 rounded-control bg-main-800 px-2.5 py-2 text-[12px] text-main-400">
                  <Icon
                    path={mdiAccountCircleOutline}
                    size={16}
                    className="flex-none text-main-500"
                  />
                  <span className="flex-1">
                    Нет привязанных аккаунтов для {ADAPTER_LABELS[vm.adapter]}.
                  </span>
                </div>
              ) : (
                <div className="flex gap-2">
                  <div className="flex min-w-0 flex-1 flex-wrap items-center gap-2.5">
                    <Icon
                      path={mdiAccountCircleOutline}
                      size={16}
                      className="flex-none text-accent-dark"
                    />
                    <span className="min-w-48 flex-1">
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
                help="Ключ доступа из раздела «Секреты»."
                error={vm.errorOf("secretId")}
              >
                <div className="flex gap-2">
                  <span className="min-w-0 flex-1">
                    <SelectInput
                      label="API-токен"
                      value={vm.secretId ?? ""}
                      placeholder="Выберите секрет"
                      searchable
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
                  <Button tone="ghost" onClick={onManageSecrets}>
                    <Icon path={mdiKeyOutline} size={15} />
                    Секреты
                  </Button>
                </div>
              </Field>
            </>
          )}
        </div>
      )}

      <div className="flex flex-col gap-3">
        {vm.isLocal ? (
          <p className="text-[12px] text-main-400">
            Модели из папки загрузок доступны после поиска.
          </p>
        ) : null}

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

        {vm.banner === null ? null : <Alert>{vm.banner}</Alert>}
      </div>
    </section>
  );
}

export default observer(ConnectionCard);
