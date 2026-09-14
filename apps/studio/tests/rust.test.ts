import assert from "node:assert/strict";
import { join } from "node:path";
import { test } from "vitest";
import { AppError, AppErrorCode, contract } from "@zvs/shared";
import { RustCore } from "../src/host/drivers/rust/RustCore.ts";
import { loadAddon, type NativeAddon } from "../src/host/drivers/rust/addon.ts";
import { nativeAddonPath, nativeTargetTriple } from "../src/host/platform/paths.ts";
import { SystemService } from "../src/host/services/SystemService.ts";
import { createSystemHandlers } from "../src/host/ipc/system.ts";
import { createEventBus } from "../src/host/platform/events.ts";
import { STUDIO_ROOT, temporaryDirectory } from "../../../test/helpers/paths.ts";

const unusedVectorExports = {
  async vectorCall() {
    throw new Error("unused");
  },
  async vectorBeginUpsert() {
    throw new Error("unused");
  },
  async vectorCancel() {
    throw new Error("unused");
  },
  async vectorRelease() {
    throw new Error("unused");
  },
};

test("native paths resolve through resources in development and packaged layouts", () => {
  const env = { userData: "user", resources: "bundle", appRoot: STUDIO_ROOT, packaged: false };
  assert.equal(
    nativeAddonPath(env),
    join(STUDIO_ROOT, "resources/native", nativeTargetTriple(), "zvs-core.node"),
  );
  assert.equal(
    nativeAddonPath({ ...env, packaged: true }),
    join("bundle/native", nativeTargetTriple(), "zvs-core.node"),
  );
});

test("missing addon rejects with the expected path, build command and original cause", async () => {
  const temp = temporaryDirectory("studio-native-");
  try {
    const paths = { nativeAddonPath: join(temp.path, "missing.node") };
    assert.throws(
      () => loadAddon(paths),
      (error: unknown) => {
        assert.ok(error instanceof AppError);
        assert.equal(error.code, AppErrorCode.NATIVE_ERROR);
        assert.ok(error.message.includes(paths.nativeAddonPath));
        assert.ok(error.message.includes("pnpm build:rust"));
        assert.ok(error.cause instanceof Error);
        return true;
      },
    );
    await assert.rejects(RustCore.fromPaths(paths).hash(new Uint8Array()), {
      code: AppErrorCode.NATIVE_ERROR,
    });
  } finally {
    temp.dispose();
  }
});

test("wrapper maps structured native codes and unexpected failures for both methods", async () => {
  for (const code of [
    "VALIDATION_FAILED",
    "RUN_CANCELLED",
    "NOT_FOUND",
    "PERMISSION_DENIED",
    "NATIVE_ERROR",
    "future-code",
  ]) {
    const error = new Error(JSON.stringify({ code, message: "native detail" }));
    const addon: NativeAddon = {
      ...unusedVectorExports,
      async chunkText() {
        throw error;
      },
      async hashBytes() {
        throw error;
      },
    };
    const core = new RustCore(() => addon);
    for (const call of [() => core.chunk("text"), () => core.hash(new Uint8Array())]) {
      await assert.rejects(call(), {
        code: code === "future-code" ? AppErrorCode.NATIVE_ERROR : code,
        cause: error,
      });
    }
  }
  await assert.rejects(
    new RustCore(() => {
      throw new Error("broken");
    }).chunk("text"),
    { code: AppErrorCode.NATIVE_ERROR },
  );
});

test("wrapper owns binary inputs and rejects configs before napi numeric coercion", async () => {
  let received: Buffer | undefined;
  const core = new RustCore(() => ({
    ...unusedVectorExports,
    async chunkText() {
      throw new Error("must not call");
    },
    async hashBytes(bytes) {
      received = bytes;
      return "hash";
    },
  }));
  const source = new Uint8Array([0, 255, 128]);
  assert.equal(await core.hash(source), "hash");
  source.fill(1);
  assert.deepEqual(received, Buffer.from([0, 255, 128]));
  for (const config of [
    { size: 0, overlap: 0 },
    { size: 1.5, overlap: 0 },
    { size: 2 ** 32, overlap: 0 },
    { size: 2, overlap: -1 },
    { size: 2, overlap: 2 },
  ]) {
    await assert.rejects(core.chunk("text", config), { code: AppErrorCode.VALIDATION_FAILED });
  }
});

test("nativePing delegates through the service port and satisfies the IPC contract", async () => {
  const events = createEventBus();
  const system = new SystemService({
    async chunk(text) {
      assert.equal(text, "sample");
      return [{ text, byteStart: 0, byteEnd: 6, tokenCount: 1 }];
    },
    async hash() {
      throw new Error("unused");
    },
  });
  try {
    const handlers = createSystemHandlers({
      system,
      events,
      runs: {
        start() {
          throw new Error("unused");
        },
      },
    });
    const output = await handlers["system.nativePing"]({ text: "sample" });
    assert.deepEqual(contract["system.nativePing"].output.parse(output), { count: 1 });
    assert.equal(contract["system.nativePing"].input.safeParse({ text: 3 }).success, false);
  } finally {
    events.dispose();
  }
});
