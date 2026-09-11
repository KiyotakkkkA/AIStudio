import { beforeEach, expect, test, vi } from "vitest";
import { createSessionGateway, sessionGateways } from "../src/host/browser/sessionGateway.ts";
import { identityProbe } from "../src/host/drivers/ai/identity/registry.ts";
import { accountSession } from "../src/host/drivers/ai/identity/AccountSession.ts";

const electron = vi.hoisted(() => {
  const profile = {
    getUserAgent: () => "test-agent",
    fetch: vi.fn(),
    webRequest: { onSendHeaders: vi.fn() },
  };
  return { profile, session: { fromPartition: vi.fn(() => profile) } };
});
vi.mock("electron", () => ({ session: electron.session }));

beforeEach(() => vi.clearAllMocks());

function observe(
  url: string,
  authorization: string,
  pageUrl = url,
  profile: unknown = electron.profile,
  fromPage = true,
): void {
  const listener = electron.profile.webRequest.onSendHeaders.mock.calls.at(-1)![1] as (
    details: Electron.OnSendHeadersListenerDetails,
  ) => void;
  listener({
    url,
    requestHeaders: { Authorization: authorization },
    ...(fromPage
      ? { webContents: { getURL: () => pageUrl, isDestroyed: () => false, session: profile } }
      : {}),
  } as unknown as Electron.OnSendHeadersListenerDetails);
}

for (const family of ["qwen-web", "deepseek-web"] as const) {
  test(`${family} detects existing and newly authenticated browser sessions`, async () => {
    const gateway = createSessionGateway();
    const probe = identityProbe(family);
    const payload =
      family === "qwen-web"
        ? { id: "user-1", name: "Person" }
        : { code: 0, data: { biz_code: 0, biz_data: { id: "user-1" } } };
    electron.profile.fetch.mockImplementation(async (_url, request) =>
      request.headers.authorization === "Bearer browser-token"
        ? Response.json(payload)
        : Response.json({}, { status: 401 }),
    );

    await expect(probe.probe(gateway, AbortSignal.timeout(1000))).rejects.toMatchObject({
      code: "PROVIDER_SESSION_EXPIRED",
    });
    // The website sends this on page load for an existing login, or after a new login.
    observe(probe.endpoint, "Bearer browser-token");
    await expect(probe.probe(gateway, AbortSignal.timeout(1000))).resolves.toMatchObject({
      identity: { externalId: "user-1" },
      credential: { token: "browser-token", tokenType: "Bearer" },
    });
    expect(electron.profile.fetch.mock.calls.at(-1)![1]).toMatchObject({
      credentials: "include",
      headers: { authorization: "Bearer browser-token" },
    });
    // A captured token must still be validated by the vendor.
    observe(probe.endpoint, "Bearer expired-token");
    await expect(probe.probe(gateway, AbortSignal.timeout(1000))).rejects.toMatchObject({
      code: "PROVIDER_SESSION_EXPIRED",
    });
  });
}

test("tokens stay scoped to the vendor and browser partition, and host probes cannot overwrite them", () => {
  const gateway = createSessionGateway();
  const endpoint = identityProbe("qwen-web").endpoint;
  observe(endpoint, "Bearer original");
  observe(endpoint, "Bearer hostile", "https://untrusted.example/");
  observe(endpoint, "Bearer other-profile", endpoint, {});
  observe(endpoint, "Bearer host-probe", endpoint, electron.profile, false);
  observe(endpoint, "Basic unsupported");
  observe("https://untrusted.example/api", "Bearer unrelated");
  expect(gateway.accountToken?.(endpoint)).toEqual({ token: "original", tokenType: "Bearer" });
  expect(gateway.accountToken?.(identityProbe("deepseek-web").endpoint)).toBeNull();
  expect(gateway.accountToken?.("https://untrusted.example/")).toBeNull();
});

test("the shared factory installs one observer per partition", () => {
  const sessions = sessionGateways();
  expect(sessions("persist:browser-work")).toBe(sessions("persist:browser-work"));
  expect(electron.profile.webRequest.onSendHeaders).toHaveBeenCalledTimes(1);
});

test("saved credentials survive a restart and newer browser credentials take precedence", async () => {
  const gateway = createSessionGateway();
  const endpoint = identityProbe("qwen-web").endpoint;
  const resolve = vi.fn(async () => "saved-token");
  const restored = await accountSession(
    { adapter: "qwen-web", tokenSecretId: "saved-secret" },
    gateway,
    { resolve },
  );
  expect(resolve).toHaveBeenCalledWith("saved-secret");
  expect(restored.accountToken?.(endpoint)).toEqual({ token: "saved-token", tokenType: "Bearer" });
  expect(restored.accountToken?.("https://chat.deepseek.com/api/v0/users/current")).toBeNull();
  expect(restored.accountToken?.("https://untrusted.example/")).toBeNull();
  observe(endpoint, "Bearer rotated-token");
  expect(restored.accountToken?.(endpoint)).toEqual({
    token: "rotated-token",
    tokenType: "Bearer",
  });
});
