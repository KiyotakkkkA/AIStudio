export interface SessionRequest {
  readonly method: string;
  readonly headers: Readonly<Record<string, string>>;
  readonly body?: string;
  readonly signal: AbortSignal;
}

export interface SessionGateway {
  readonly partition: string;
  userAgent(): string;
  fetch(url: string, request: SessionRequest): Promise<Response>;
}

export type SessionGatewayFactory = (partition: string) => SessionGateway;
