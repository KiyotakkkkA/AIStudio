import { session } from "electron";
import type {
  SessionGateway,
  SessionGatewayFactory,
  SessionRequest,
} from "../drivers/ai/transport/SessionGateway.ts";
import { BROWSER_PARTITION } from "./policy.ts";
import type { AccountToken } from "../drivers/ai/transport/AccountTransport.ts";

const ACCOUNT_ORIGINS = ["https://chat.qwen.ai", "https://chat.deepseek.com"];

export function createSessionGateway(partition: string = BROWSER_PARTITION): SessionGateway {
  const profile = session.fromPartition(partition);
  const tokens = new Map<string, AccountToken>();
  profile.webRequest.onSendHeaders(
    { urls: ACCOUNT_ORIGINS.map((origin) => `${origin}/*`) },
    (details) => {
      const contents = details.webContents;
      if (!contents || contents.isDestroyed() || contents.session !== profile) return;
      const origin = new URL(details.url).origin;
      if (!ACCOUNT_ORIGINS.includes(origin)) return;
      const pageUrl = contents.getURL();
      if (!pageUrl || new URL(pageUrl).origin !== origin) return;
      const authorization = Object.entries(details.requestHeaders).find(
        ([name]) => name.toLowerCase() === "authorization",
      )?.[1];
      const match = authorization?.match(/^Bearer\s+(\S+)$/i);
      if (match?.[1]) tokens.set(origin, { token: match[1], tokenType: "Bearer" });
    },
  );
  return {
    partition,
    userAgent: () => profile.getUserAgent(),
    accountToken: (url) => tokens.get(new URL(url).origin) ?? null,
    fetch: async (url: string, request: SessionRequest): Promise<Response> =>
      await profile.fetch(url, {
        method: request.method,
        headers: request.headers,
        signal: request.signal,
        credentials: "include",
        ...(request.body === undefined ? {} : { body: request.body }),
      }),
  };
}

export function sessionGateways(): SessionGatewayFactory {
  const cache = new Map<string, SessionGateway>();
  return (partition: string): SessionGateway => {
    const existing = cache.get(partition);
    if (existing !== undefined) return existing;
    const gateway = createSessionGateway(partition);
    cache.set(partition, gateway);
    return gateway;
  };
}
