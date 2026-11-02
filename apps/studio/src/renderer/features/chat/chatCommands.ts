import type {
  ChatCommand,
  ChatCommandDto,
  IntegrationAccountDto,
  IntegrationId,
  IntegrationKind,
} from "@zvs/shared";
import { plural } from "../connections/connectionPresentation";
import {
  PLUGIN_DESCRIPTIONS,
  PLUGIN_KINDS,
  PLUGIN_LABELS,
  pluginCommand,
} from "../plugins/pluginPresentation";

export const COMMANDS_LABEL = "Команды и плагины";
export const ACCOUNTS_LABEL = "Аккаунты плагина";
export const REMOVE_COMMAND = "Убрать команду";
export const REMOVE_PLUGIN = "Убрать плагин";

export function commandText(command: ChatCommand): string {
  return `/${command}`;
}

export type MenuEntry =
  | {
      readonly type: "command";
      readonly key: string;
      readonly label: string;
      readonly description: string;
      readonly command: ChatCommand;
    }
  | {
      readonly type: "plugin";
      readonly key: string;
      readonly label: string;
      readonly description: string;
      readonly plugin: IntegrationKind;
      readonly accounts: readonly IntegrationAccountDto[];
    }
  | {
      readonly type: "account";
      readonly key: string;
      readonly label: string;
      readonly description: string;
      readonly account: IntegrationAccountDto;
    };

export interface MenuInput {
  readonly commands: readonly ChatCommandDto[];
  readonly accounts: readonly IntegrationAccountDto[];
  readonly attachedCommands: readonly ChatCommand[];
  readonly attachedPlugins: readonly IntegrationId[];
  readonly text: string;
  readonly picking: IntegrationKind | null;
}

const SLASH_TOKEN = /(^|\s)\/([^\s/]*)$/u;

export function slashQuery(text: string): string | null {
  const match = SLASH_TOKEN.exec(text);
  return match === null ? null : (match[2] ?? "").toLowerCase();
}

export function withoutSlashToken(text: string): string {
  return text.replace(SLASH_TOKEN, "$1");
}

export function attachableAccounts(
  accounts: readonly IntegrationAccountDto[],
  attached: readonly IntegrationId[],
  kind: IntegrationKind,
): IntegrationAccountDto[] {
  return accounts.filter(
    (account) => account.kind === kind && account.enabled && !attached.includes(account.id),
  );
}

function accountsWord(count: number): string {
  return plural(count, "аккаунт", "аккаунта", "аккаунтов");
}

export function menuEntries(input: MenuInput): MenuEntry[] {
  if (input.picking !== null) {
    return attachableAccounts(input.accounts, input.attachedPlugins, input.picking).map(
      (account) => ({
        type: "account",
        key: account.id,
        label: account.name,
        description: PLUGIN_LABELS[account.kind],
        account,
      }),
    );
  }
  const query = slashQuery(input.text);
  if (query === null) return [];
  const commands: MenuEntry[] = input.commands
    .filter(
      (item) => !input.attachedCommands.includes(item.command) && item.command.startsWith(query),
    )
    .map((item) => ({
      type: "command",
      key: item.command,
      label: commandText(item.command),
      description: item.description,
      command: item.command,
    }));
  const plugins: MenuEntry[] = PLUGIN_KINDS.filter((kind) => kind.startsWith(query)).flatMap(
    (kind) => {
      const accounts = attachableAccounts(input.accounts, input.attachedPlugins, kind);
      const only = accounts[0];
      if (only === undefined) return [];
      const description =
        accounts.length === 1
          ? `${PLUGIN_DESCRIPTIONS[kind]} · ${only.name}`
          : `${PLUGIN_DESCRIPTIONS[kind]} · ${accountsWord(accounts.length)}`;
      return [
        {
          type: "plugin",
          key: kind,
          label: pluginCommand(kind),
          description,
          plugin: kind,
          accounts,
        } satisfies MenuEntry,
      ];
    },
  );
  return [...commands, ...plugins];
}

export interface ParsedCommands {
  readonly commands: readonly ChatCommand[];
  readonly text: string;
}

export function parseCommands(
  commands: readonly ChatCommandDto[],
  text: string,
): ParsedCommands | null {
  let rest = text.trim();
  const found: ChatCommand[] = [];
  for (;;) {
    const match = /^\/(\S+)(?:\s+([\s\S]*))?$/u.exec(rest);
    if (match === null) break;
    const name = (match[1] ?? "").toLowerCase();
    const known = commands.find((item) => item.command === name);
    if (known === undefined) break;
    if (!found.includes(known.command)) found.push(known.command);
    rest = (match[2] ?? "").trim();
  }
  return found.length === 0 ? null : { commands: found, text: rest };
}
