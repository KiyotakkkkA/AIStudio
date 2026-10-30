import { expect, test } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import { AppErrorCode, contract, type Contract, type ConversationId } from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import ConversationExport, {
  EXPORT_FAILED,
  EXPORT_NO_ACCESS,
  EXPORT_SAVED,
} from "../../src/renderer/features/chat/ConversationExport.ts";

const ID = "0199aa11-2222-7111-8111-111111111111" as ConversationId;

test("a saved export reports the path and a cancelled dialog reports nothing", async () => {
  let path: string | undefined = "C:\\Users\\me\\Диалог.md";
  const bridge = createFakeBridge<Contract>().handle("chat.conversations.export", () =>
    path === undefined ? {} : { path },
  );
  const exports = new ConversationExport(createIpcClient(contract, bridge));

  await exports.export(ID, "markdown");
  expect(exports.notice).toEqual({ tone: "info", title: EXPORT_SAVED, detail: path });
  expect(bridge.calls.at(-1)?.payload).toEqual({ id: ID, format: "markdown" });

  path = undefined;
  await exports.export(ID, "json");
  expect(exports.notice).toBeNull();
  expect(exports.busy).toBe(false);
});

test("failures map error codes to copy and the notice can be dismissed", async () => {
  const denied = new ConversationExport(
    createIpcClient(
      contract,
      createFakeBridge<Contract>().fail(
        "chat.conversations.export",
        AppErrorCode.PERMISSION_DENIED,
        "x",
      ),
    ),
  );
  await denied.export(ID, "json");
  expect(denied.notice).toEqual({ tone: "error", title: EXPORT_NO_ACCESS });
  denied.dismiss();
  expect(denied.notice).toBeNull();

  const broken = new ConversationExport(
    createIpcClient(
      contract,
      createFakeBridge<Contract>().fail("chat.conversations.export", AppErrorCode.UNKNOWN, "x"),
    ),
  );
  await broken.export(ID, "json");
  expect(broken.notice?.title).toBe(EXPORT_FAILED);
});

test("a second export waits for the first", async () => {
  let release: () => void = () => undefined;
  const bridge = createFakeBridge<Contract>().handle(
    "chat.conversations.export",
    () =>
      new Promise<{ path: string }>((resolve) => {
        release = () => resolve({ path: "C:\\a.json" });
      }),
  );
  const exports = new ConversationExport(createIpcClient(contract, bridge));
  const first = exports.export(ID, "json");
  await exports.export(ID, "markdown");
  release();
  await first;
  expect(bridge.calls.filter((call) => call.channel === "chat.conversations.export")).toHaveLength(
    1,
  );
});
