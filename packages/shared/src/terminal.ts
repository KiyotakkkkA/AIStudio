import { z } from "zod";
import { ConversationId, StreamId, TerminalId } from "./primitives/branded.js";

export const TERMINAL_STEP_DOMAIN = "terminal";
export const TERMINAL_INPUT_MAX = 65_536;
export const TERMINAL_SCROLLBACK_MAX = 262_144;
export const TERMINAL_TITLE_MAX = 80;
export const TERMINAL_COLS_MAX = 1000;
export const TERMINAL_ROWS_MAX = 500;
export const TERMINAL_SESSIONS_MAX = 8;

export const TerminalShell = z.enum(["powershell", "pwsh", "cmd", "gitBash", "wsl"]);
export type TerminalShell = z.infer<typeof TerminalShell>;

export const TerminalStatus = z.enum(["running", "exited"]);
export type TerminalStatus = z.infer<typeof TerminalStatus>;

const Cols = z.number().int().min(2).max(TERMINAL_COLS_MAX);
const Rows = z.number().int().min(1).max(TERMINAL_ROWS_MAX);
const Title = z.string().min(1).max(TERMINAL_TITLE_MAX);

export const TerminalSessionDto = z.object({
  id: TerminalId,
  conversationId: ConversationId,
  shell: TerminalShell,
  title: Title,
  cwd: z.string().min(1),
  status: TerminalStatus,
  exitCode: z.number().int().nullable(),
  streamId: StreamId,
  cols: Cols,
  rows: Rows,
  createdAt: z.number().int().nonnegative(),
});
export type TerminalSessionDto = z.infer<typeof TerminalSessionDto>;

export const TerminalCreateInput = z.object({
  conversationId: ConversationId,
  shell: TerminalShell.nullable(),
  cols: Cols,
  rows: Rows,
});
export type TerminalCreateInput = z.infer<typeof TerminalCreateInput>;

export const TerminalRef = z.object({ id: TerminalId });
export type TerminalRef = z.infer<typeof TerminalRef>;

export const TerminalWriteInput = z.object({
  id: TerminalId,
  data: z.string().min(1).max(TERMINAL_INPUT_MAX),
});
export type TerminalWriteInput = z.infer<typeof TerminalWriteInput>;

export const TerminalResizeInput = z.object({ id: TerminalId, cols: Cols, rows: Rows });
export type TerminalResizeInput = z.infer<typeof TerminalResizeInput>;

export const TerminalRenameInput = z.object({ id: TerminalId, title: Title });
export type TerminalRenameInput = z.infer<typeof TerminalRenameInput>;

export const TerminalAttachDto = z.object({
  session: TerminalSessionDto,
  scrollback: z.string().max(TERMINAL_SCROLLBACK_MAX),
  nextSeq: z.number().int().nonnegative(),
});
export type TerminalAttachDto = z.infer<typeof TerminalAttachDto>;

export const TerminalStatusDto = z.object({
  domain: z.literal(TERMINAL_STEP_DOMAIN),
  id: TerminalId,
  status: TerminalStatus,
  exitCode: z.number().int().nullable(),
});
export type TerminalStatusDto = z.infer<typeof TerminalStatusDto>;
