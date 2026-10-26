import type { ChatCommand, ChatCommandDto } from "@zvs/shared";

export const COMMANDS_LABEL = "Команды";
export const REMOVE_COMMAND = "Убрать команду";

export function commandText(command: ChatCommand): string {
  return `/${command}`;
}

export function matchCommands(commands: readonly ChatCommandDto[], text: string): ChatCommandDto[] {
  if (!/^\/\S*$/.test(text)) return [];
  const query = text.slice(1).toLowerCase();
  return commands.filter((item) => item.command.startsWith(query));
}

export interface ParsedCommand {
  readonly command: ChatCommand;
  readonly text: string;
}

export function parseCommand(
  commands: readonly ChatCommandDto[],
  text: string,
): ParsedCommand | null {
  const match = /^\/(\S+)(?:\s+([\s\S]*))?$/.exec(text.trim());
  if (!match) return null;
  const name = match[1]!.toLowerCase();
  const known = commands.find((item) => item.command === name);
  return known ? { command: known.command, text: (match[2] ?? "").trim() } : null;
}
