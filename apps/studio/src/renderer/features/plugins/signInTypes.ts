import type { IntegrationAccountDto, IntegrationConfig, IntegrationKind } from "@zvs/shared";
import type { VendorMarkStyle } from "../providers/accountPresentation";
import { MAIL_PRESET_MARKS, PLUGIN_MARKS } from "./pluginPresentation";

export type SignInTypeId =
  | "google"
  | "yandex"
  | "mailru"
  | "imap"
  | "githubToken"
  | "githubOauth"
  | "gitlabToken"
  | "gitlabOauth"
  | "telegramBot";

type FormValues = Readonly<Record<string, string | boolean>>;

export interface SignInType {
  readonly id: SignInTypeId;
  readonly kind: IntegrationKind;
  readonly label: string;
  readonly chip: string;
  readonly mark: VendorMarkStyle;
  readonly fixed: Readonly<Record<string, string>>;
  readonly hidden: readonly string[];
  readonly required: readonly string[];
  readonly browser: boolean;
  readonly hiddenWhen?: (values: FormValues) => readonly string[];
}

export const SIGNED_IN_FIELD = "oauthToken";

const ALWAYS_HIDDEN = [SIGNED_IN_FIELD];

const MAIL_MANUAL = [
  "imapHost",
  "imapPort",
  "imapSecurity",
  "imapUser",
  "imapPassword",
  "smtpHost",
  "smtpPort",
  "smtpSecurity",
  "smtpReuseLogin",
  "smtpUser",
  "smtpPassword",
];

const OAUTH_CLIENT = ["oauthClientId", "oauthClientSecret"];

const SMTP_LOGIN = ["smtpUser", "smtpPassword"];

function mailPreset(id: "google" | "yandex" | "mailru", label: string): SignInType {
  return {
    id,
    kind: "mail",
    label,
    chip: label,
    mark: MAIL_PRESET_MARKS[id],
    fixed: { protocol: id },
    hidden: MAIL_MANUAL,
    required: ["oauthClientId"],
    browser: true,
  };
}

function forgeTypes(kind: "github" | "gitlab", label: string): readonly SignInType[] {
  return [
    {
      id: kind === "github" ? "githubToken" : "gitlabToken",
      kind,
      label: "Токен доступа",
      chip: "Токен",
      mark: PLUGIN_MARKS[kind],
      fixed: { auth: "token" },
      hidden: OAUTH_CLIENT,
      required: ["token"],
      browser: false,
    },
    {
      id: kind === "github" ? "githubOauth" : "gitlabOauth",
      kind,
      label: `Вход через ${label}`,
      chip: "Браузер",
      mark: PLUGIN_MARKS[kind],
      fixed: { auth: "oauth" },
      hidden: ["token"],
      required: [],
      browser: true,
    },
  ];
}

export const SIGN_IN_TYPES: Record<IntegrationKind, readonly SignInType[]> = {
  mail: [
    mailPreset("google", "Google"),
    mailPreset("yandex", "Яндекс"),
    mailPreset("mailru", "Mail.ru"),
    {
      id: "imap",
      kind: "mail",
      label: "Другой сервер (IMAP / SMTP)",
      chip: "IMAP",
      mark: MAIL_PRESET_MARKS.imap,
      fixed: { protocol: "imap" },
      hidden: OAUTH_CLIENT,
      required: ["imapHost", "imapUser", "imapPassword"],
      browser: false,
      hiddenWhen: (values) => (values.smtpReuseLogin === false ? [] : SMTP_LOGIN),
    },
  ],
  github: forgeTypes("github", "GitHub"),
  gitlab: forgeTypes("gitlab", "GitLab"),
  telegram: [
    {
      id: "telegramBot",
      kind: "telegram",
      label: "Токен бота",
      chip: "Бот",
      mark: PLUGIN_MARKS.telegram,
      fixed: {},
      hidden: [],
      required: [],
      browser: false,
    },
  ],
};

export function signInTypeOf(account: Pick<IntegrationAccountDto, "kind" | "config">): SignInType {
  const types = SIGN_IN_TYPES[account.kind];
  const match = types.find((type) =>
    Object.entries(type.fixed).every(([key, value]) => account.config[key] === value),
  );
  return match ?? types.find((type) => type.id === "imap") ?? (types[0] as SignInType);
}

export function hiddenKeys(type: SignInType, values: FormValues): ReadonlySet<string> {
  return new Set([
    ...ALWAYS_HIDDEN,
    ...Object.keys(type.fixed),
    ...type.hidden,
    ...(type.hiddenWhen?.(values) ?? []),
  ]);
}

export function isSignedIn(config: IntegrationConfig): boolean {
  const value = config[SIGNED_IN_FIELD];
  return typeof value === "object" && value !== null && "secretId" in value;
}

export function accountMark(
  account: Pick<IntegrationAccountDto, "kind" | "config">,
): VendorMarkStyle {
  return signInTypeOf(account).mark;
}
