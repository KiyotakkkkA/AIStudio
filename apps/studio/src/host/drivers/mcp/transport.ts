import type { McpTransport as McpTransportKind } from "@zvs/shared";
import type { JsonRpcOutgoing } from "./protocol.ts";

export type McpLogLevel = "debug" | "info" | "warn" | "error";

export interface McpTransportHandlers {
  /** One decoded frame off the wire, still untrusted — the client validates it. */
  message(value: unknown): void;
  log(level: McpLogLevel, line: string): void;
  /** The connection ended for any reason: exit, error, or our own `close`. */
  closed(reason: string): void;
}

/**
 * The port the client speaks through. Everything that knows about processes, sockets or HTTP
 * lives behind it, so a test drives a real handshake without a real server.
 */
export interface McpTransport {
  readonly kind: McpTransportKind;
  open(handlers: McpTransportHandlers): Promise<void>;
  send(message: JsonRpcOutgoing): Promise<void>;
  close(): Promise<void>;
  /** Called once the handshake settles, so later requests can carry the negotiated version. */
  acceptProtocolVersion?(version: string): void;
}
