import { makeAutoObservable } from "mobx";
import {
  PROVIDER_CAPABILITIES,
  type AccountDto,
  type AccountId,
  type AdapterDescriptorDto,
  type AdapterFamily,
  type AuthMode,
  type CreateProviderInput,
  type ProbeResultDto,
  type ProviderCapability,
  type ProviderConnectionInput,
  type ProviderDto,
  type ProviderId,
  type ProviderKind,
  type ProviderSettingsDto,
  type SecretId,
  type TunableParameter,
  type UpdateProviderInput,
} from "@zvs/shared";
import { suggestedBaseUrl } from "./providerPresentation";

export const SETTING_DEFAULTS = {
  temperature: 0.7,
  topK: 40,
  topP: 0.95,
  maxOutputTokens: 4096,
  timeoutSeconds: 60,
} as const;

export const SETTING_BOUNDS = {
  temperature: { min: 0, max: 2, step: 0.01 },
  topK: { min: 1, max: 4096, step: 1 },
  topP: { min: 0, max: 1, step: 0.01 },
  maxOutputTokens: { min: 1, max: 1_000_000 },
  timeoutSeconds: { min: 1, max: 600 },
} as const;

export type SettingKey = keyof typeof SETTING_DEFAULTS;

const FALLBACK_DESCRIPTOR: AdapterDescriptorDto = {
  family: "openai-compatible",
  authModes: ["api"],
  streaming: true,
  liveModelList: true,
  embedding: true,
  image: false,
  honours: { temperature: true, topK: false, topP: true, maxOutputTokens: true },
  implemented: true,
};

interface Snapshot {
  readonly connection: string;
  readonly name: string;
  readonly settings: string;
  readonly enabled: boolean;
}

export class ProviderFormVm {
  readonly providerId: ProviderId | null;

  kind: ProviderKind = "ollama";
  adapter: AdapterFamily = "openai-compatible";
  authMode: AuthMode = "api";
  name = "";
  baseUrl = "";
  secretId: SecretId | null = null;
  accountId: AccountId | null = null;
  capabilities: ProviderCapability[] = [];
  settings: Record<SettingKey, number | null> = {
    temperature: null,
    topK: null,
    topP: null,
    maxOutputTokens: null,
    timeoutSeconds: null,
  };
  enabled = true;
  selectedModelIds: string[] = [];

  errors: Record<string, string> = {};
  banner: string | null = null;
  probeResult: ProbeResultDto | null = null;

  private adapters: readonly AdapterDescriptorDto[];
  private accounts: readonly AccountDto[];
  private readonly initial: Snapshot;

  constructor(options: {
    readonly adapters: readonly AdapterDescriptorDto[];
    readonly accounts: readonly AccountDto[];
    readonly capability: ProviderCapability;
    readonly provider?: ProviderDto | null;
  }) {
    this.adapters = options.adapters;
    this.accounts = options.accounts;
    const provider = options.provider ?? null;
    this.providerId = provider?.id ?? null;

    if (provider === null) {
      this.capabilities = [options.capability];
      this.baseUrl = suggestedBaseUrl(this.kind, this.adapter);
    } else {
      this.kind = provider.kind;
      this.adapter = provider.adapter;
      this.authMode = provider.authMode;
      this.name = provider.name;
      this.baseUrl = provider.baseUrl;
      this.secretId = provider.secretId;
      this.accountId = provider.accountId;
      this.capabilities = [...provider.capabilities];
      this.enabled = provider.enabled;
      this.selectedModelIds = [
        ...(provider.settings.selectedModelIds ??
          provider.models
            .filter((model) => model.id === provider.defaultModelId)
            .map((model) => model.externalId)),
      ];
      this.settings = {
        temperature: provider.settings.temperature ?? null,
        topK: provider.settings.topK ?? null,
        topP: provider.settings.topP ?? null,
        maxOutputTokens: provider.settings.maxOutputTokens ?? null,
        timeoutSeconds: provider.settings.timeoutSeconds ?? null,
      };
    }

    this.initial = this.snapshot();
    makeAutoObservable<ProviderFormVm, "initial" | "adapters" | "accounts">(
      this,
      { initial: false, adapters: false, accounts: false },
      { autoBind: true },
    );
  }

  get isNew(): boolean {
    return this.providerId === null;
  }

  get descriptor(): AdapterDescriptorDto {
    return this.adapters.find((entry) => entry.family === this.adapter) ?? FALLBACK_DESCRIPTOR;
  }

  get adapterOptions(): readonly AdapterDescriptorDto[] {
    return this.adapters;
  }

  get accountOptions(): readonly AccountDto[] {
    return this.accounts.filter((account) => account.adapter === this.adapter);
  }

  get account(): AccountDto | null {
    return this.accounts.find((entry) => entry.id === this.accountId) ?? null;
  }

  get usesAccount(): boolean {
    return this.authMode === "account";
  }

  get dirty(): boolean {
    const current = this.snapshot();
    return (
      current.connection !== this.initial.connection ||
      current.name !== this.initial.name ||
      current.settings !== this.initial.settings ||
      current.enabled !== this.initial.enabled
    );
  }

  honours(parameter: TunableParameter): boolean {
    return this.descriptor.honours[parameter] === true;
  }

  authModeDisabledReason(mode: AuthMode): string | null {
    if (this.descriptor.authModes.includes(mode)) return null;
    return mode === "account"
      ? `Семейство ${this.adapter} работает только по API-ключу.`
      : `Семейство ${this.adapter} работает только через привязанный аккаунт.`;
  }

  capabilityDisabledReason(capability: ProviderCapability): string | null {
    if (capability === "embedding" && !this.descriptor.embedding) {
      return `Семейство ${this.adapter} не отдаёт эмбеддинги.`;
    }
    if (capability === "image" && !this.descriptor.image) {
      return `Семейство ${this.adapter} не генерирует изображения.`;
    }
    return null;
  }

  setKind(kind: ProviderKind): void {
    if (kind === this.kind) return;
    const previous = suggestedBaseUrl(this.kind, this.adapter);
    this.kind = kind;
    if (this.baseUrl.trim().length === 0 || this.baseUrl === previous) {
      this.baseUrl = suggestedBaseUrl(kind, this.adapter);
    }
    this.clearError("baseUrl");
    this.forgetProbe();
  }

  setAdapter(adapter: AdapterFamily): void {
    if (adapter === this.adapter) return;
    const previous = suggestedBaseUrl(this.kind, this.adapter);
    this.adapter = adapter;
    if (this.baseUrl.trim().length === 0 || this.baseUrl === previous) {
      this.baseUrl = suggestedBaseUrl(this.kind, adapter);
    }
    const modes = this.descriptor.authModes;
    const fallback = modes[0];
    if (!modes.includes(this.authMode) && fallback !== undefined) this.authMode = fallback;
    this.capabilities = this.capabilities.filter(
      (capability) => this.capabilityDisabledReason(capability) === null,
    );
    this.clearError("adapter");
    this.forgetProbe();
  }

  setAuthMode(mode: AuthMode): void {
    if (mode === this.authMode) return;
    if (this.authModeDisabledReason(mode) !== null) return;
    this.authMode = mode;
    this.clearError("secretId");
    this.clearError("accountId");
    this.forgetProbe();
  }

  setName(name: string): void {
    this.name = name;
    this.clearError("name");
  }

  setBaseUrl(baseUrl: string): void {
    this.baseUrl = baseUrl;
    this.clearError("baseUrl");
    this.forgetProbe();
  }

  setSecretId(secretId: SecretId | null): void {
    this.secretId = secretId;
    this.clearError("secretId");
    this.forgetProbe();
  }

  setAccountId(accountId: AccountId | null): void {
    this.accountId = accountId;
    this.clearError("accountId");
    this.forgetProbe();
  }

  toggleCapability(capability: ProviderCapability): void {
    if (this.capabilityDisabledReason(capability) !== null) return;
    this.capabilities = this.capabilities.includes(capability)
      ? this.capabilities.filter((entry) => entry !== capability)
      : [...this.capabilities, capability].sort(
          (left, right) =>
            PROVIDER_CAPABILITIES.indexOf(left) - PROVIDER_CAPABILITIES.indexOf(right),
        );
    this.clearError("capabilities");
  }

  setEnabled(enabled: boolean): void {
    this.enabled = enabled;
  }

  toggleModel(externalId: string): void {
    this.selectedModelIds = this.selectedModelIds.includes(externalId)
      ? this.selectedModelIds.filter((id) => id !== externalId)
      : [...this.selectedModelIds, externalId];
  }

  settingOf(key: SettingKey): number {
    return this.settings[key] ?? SETTING_DEFAULTS[key];
  }

  isSettingExplicit(key: SettingKey): boolean {
    return this.settings[key] !== null;
  }

  setSetting(key: SettingKey, value: number | null): void {
    this.settings = { ...this.settings, [key]: value };
    this.clearError(`settings.${key}`);
  }

  setProbeResult(result: ProbeResultDto): void {
    this.probeResult = result;
  }

  forgetProbe(): void {
    this.probeResult = null;
  }

  setErrors(errors: Record<string, string>): void {
    this.errors = { ...errors };
  }

  setBanner(banner: string | null): void {
    this.banner = banner;
  }

  errorOf(key: string): string | undefined {
    return this.errors[key];
  }

  validate(): boolean {
    const errors: Record<string, string> = {};
    const name = this.name.trim();
    if (name.length === 0) errors.name = "Укажите название подключения.";
    else if (name.length > 128) errors.name = "Не длиннее 128 символов.";

    const baseUrl = this.effectiveBaseUrl();
    if (baseUrl.length === 0) errors.baseUrl = "Укажите базовый адрес.";
    else if (!isUrl(baseUrl)) errors.baseUrl = "Нужен полный адрес, например https://host/api.";

    if (this.capabilities.length === 0) errors.capabilities = "Выберите хотя бы одну возможность.";
    if (!this.descriptor.implemented) {
      errors.adapter = `Семейство ${this.adapter} ещё не реализовано.`;
    }
    if (this.authModeDisabledReason(this.authMode) !== null) {
      errors.authMode = this.authModeDisabledReason(this.authMode) ?? "";
    }
    if (this.authMode === "api" && this.secretId === null) {
      errors.secretId = "Выберите секрет с учётными данными.";
    }
    if (this.authMode === "account" && this.accountId === null) {
      errors.accountId = "Свяжите аккаунт вендора.";
    }

    this.errors = errors;
    return Object.keys(errors).length === 0;
  }

  effectiveBaseUrl(): string {
    const typed = this.baseUrl.trim();
    return typed.length > 0 ? typed : suggestedBaseUrl(this.kind, this.adapter);
  }

  settingsPayload(): ProviderSettingsDto {
    const payload: ProviderSettingsDto = { selectedModelIds: [...this.selectedModelIds] };
    if (this.settings.temperature !== null) payload.temperature = this.settings.temperature;
    if (this.settings.topK !== null) payload.topK = this.settings.topK;
    if (this.settings.topP !== null) payload.topP = this.settings.topP;
    if (this.settings.maxOutputTokens !== null) {
      payload.maxOutputTokens = this.settings.maxOutputTokens;
    }
    if (this.settings.timeoutSeconds !== null) {
      payload.timeoutSeconds = this.settings.timeoutSeconds;
    }
    return payload;
  }

  toConnectionInput(): ProviderConnectionInput {
    return {
      kind: this.kind,
      adapter: this.adapter,
      authMode: this.authMode,
      baseUrl: this.effectiveBaseUrl(),
      secretId: this.authMode === "api" ? this.secretId : null,
      accountId: this.authMode === "account" ? this.accountId : null,
      capabilities: [...this.capabilities],
      settings: this.settingsPayload(),
    };
  }

  toCreateInput(): CreateProviderInput {
    return { ...this.toConnectionInput(), name: this.name.trim(), enabled: this.enabled };
  }

  toUpdateInput(): UpdateProviderInput {
    if (this.providerId === null) throw new Error("Форма не привязана к провайдеру");
    return {
      id: this.providerId,
      ...this.toConnectionInput(),
      name: this.name.trim(),
      enabled: this.enabled,
    };
  }

  private clearError(key: string): void {
    if (this.errors[key] === undefined) return;
    const rest: Record<string, string> = {};
    for (const [name, message] of Object.entries(this.errors)) {
      if (name !== key) rest[name] = message;
    }
    this.errors = rest;
  }

  private snapshot(): Snapshot {
    return {
      connection: JSON.stringify([
        this.kind,
        this.adapter,
        this.authMode,
        this.baseUrl,
        this.secretId,
        this.accountId,
        this.capabilities,
      ]),
      name: this.name,
      settings: JSON.stringify([this.settings, this.selectedModelIds]),
      enabled: this.enabled,
    };
  }
}

function isUrl(value: string): boolean {
  try {
    const url = new URL(value);
    return url.protocol === "http:" || url.protocol === "https:";
  } catch {
    return false;
  }
}
