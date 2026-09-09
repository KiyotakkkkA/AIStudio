import { session } from "electron";
import type {
  SessionGateway,
  SessionGatewayFactory,
  SessionRequest,
} from "../drivers/ai/transport/SessionGateway.ts";
import { BROWSER_PARTITION } from "./policy.ts";

export function createSessionGateway(partition: string = BROWSER_PARTITION): SessionGateway {
  const profile = session.fromPartition(partition);
  return {
    partition,
    userAgent: () => profile.getUserAgent(),
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
