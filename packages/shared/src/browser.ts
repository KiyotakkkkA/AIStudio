import { z } from "zod";

const tabId = z.string().uuid();
export const BrowserBounds = z.object({
  x: z.number().finite().min(0),
  y: z.number().finite().min(0),
  width: z.number().finite().min(0),
  height: z.number().finite().min(0),
});
export const BrowserCommand = z.discriminatedUnion("action", [
  z.object({ action: z.literal("state") }),
  z.object({ action: z.literal("new"), url: z.string().max(8192).optional() }),
  z.object({ action: z.literal("navigate"), url: z.string().min(1).max(8192) }),
  z.object({ action: z.literal("switch"), id: tabId }),
  z.object({ action: z.literal("close"), id: tabId }),
  z.object({ action: z.literal("reorder"), id: tabId, index: z.number().int().min(0) }),
  z.object({ action: z.literal("back") }),
  z.object({ action: z.literal("forward") }),
  z.object({ action: z.literal("reload") }),
  z.object({ action: z.literal("sites"), visible: z.boolean() }),
  z.object({ action: z.literal("cookies") }),
  z.object({ action: z.literal("removeCookie"), id: z.string().min(1).max(32768) }),
  z.object({ action: z.literal("clearSite"), domain: z.string().min(1).max(253) }),
]);
export type BrowserCommand = z.infer<typeof BrowserCommand>;

export interface BrowserTab {
  id: string;
  title: string;
  url: string;
  loading: boolean;
  canGoBack: boolean;
  canGoForward: boolean;
  security: "https" | "http" | "none" | "error";
  error?: string;
}

export interface BrowserState {
  tabs: BrowserTab[];
  activeId: string | null;
  sitesVisible: boolean;
}

export interface BrowserCookie {
  id: string;
  domain: string;
  name: string;
  path: string;
  secure: boolean;
  httpOnly: boolean;
  session: boolean;
  expirationDate?: number;
}
