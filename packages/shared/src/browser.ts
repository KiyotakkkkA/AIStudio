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
  z.object({ action: z.literal("devtools"), visible: z.boolean() }),
  z.object({ action: z.literal("network") }),
  z.object({ action: z.literal("networkRecord"), recording: z.boolean() }),
  z.object({ action: z.literal("networkClear") }),
  z.object({ action: z.literal("networkSave") }),
  z.object({ action: z.literal("networkDetail"), id: z.string().min(1).max(256) }),
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
  devtoolsVisible: boolean;
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

export type BrowserRequestState = "pending" | "finished" | "failed";

export interface BrowserHeader {
  name: string;
  value: string;
  redacted: boolean;
}

export interface BrowserNetworkEntry {
  id: string;
  url: string;
  method: string;
  resourceType: string;
  state: BrowserRequestState;
  status?: number;
  mimeType?: string;
  startedAt: number;
  duration?: number;
  size?: number;
  fromCache: boolean;
  error?: string;
}

export interface BrowserNetworkLog {
  tabId: string | null;
  recording: boolean;
  revision: number;
  dropped: number;
  entries: BrowserNetworkEntry[];
}

export interface BrowserNetworkBody {
  text: string;
  base64: boolean;
  truncated: boolean;
}

export interface BrowserNetworkDetail {
  entry: BrowserNetworkEntry;
  statusText?: string;
  requestHeaders: BrowserHeader[];
  responseHeaders: BrowserHeader[];
  postData?: string;
  body?: BrowserNetworkBody;
}

export interface BrowserNetworkSaved {
  saved: boolean;
  fileName?: string;
}
