import { expect, test, vi } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import {
  AppErrorCode,
  contract,
  type Contract,
  type RecordingDeviceDto,
  type SpeechModelDto,
  type VoiceModelsDto,
} from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import VoiceStore, { MODELS_POLL_MS } from "../../src/renderer/features/voice/VoiceStore.ts";
import { appendDictation } from "../../src/renderer/features/voice/dictation.ts";
import {
  VOICE_DEVICE_MISSING,
  VOICE_DEVICES_FAILED,
  VOICE_SELECT_FAILED,
  VOICE_NOTHING_HEARD,
  VOICE_NOT_READY,
  VOICE_RECOGNITION_FAILED,
  VOICE_SELECT_MISSING,
  systemDeviceLabel,
} from "../../src/renderer/features/voice/voiceCopy.ts";

const REALTEK: RecordingDeviceDto = { id: "a", name: "Realtek", isDefault: true };
const USB: RecordingDeviceDto = { id: "b", name: "USB Microphone", isDefault: false };

function setup(initial: RecordingDeviceDto[]) {
  const state = { devices: initial, selectedId: null as string | null };
  const bridge = createFakeBridge<Contract>()
    .handle("voice.devices", () => ({ ...state }))
    .handle("voice.select", ({ deviceId }) => {
      state.selectedId = deviceId;
      return { ...state };
    });
  return { store: new VoiceStore(createIpcClient(contract, bridge)), state, bridge };
}

test("the host list is loaded once and refetched on demand", async () => {
  const { store, state, bridge } = setup([REALTEK]);
  await store.load();
  await store.load();
  expect(bridge.calls.filter((call) => call.channel === "voice.devices")).toHaveLength(1);
  expect(store.selectedLabel).toBe(systemDeviceLabel("Realtek"));

  state.devices = [REALTEK, USB];
  await store.refresh();
  expect(store.devices).toEqual([REALTEK, USB]);
});

test("a choice is saved by the host and an unplugged device is reported", async () => {
  const { store, state } = setup([REALTEK, USB]);
  await store.load();
  await store.select("b");
  expect(state.selectedId).toBe("b");
  expect(store.selectedLabel).toBe("USB Microphone");

  state.devices = [REALTEK];
  await store.refresh();
  expect(store.selectedLabel).toBe(VOICE_DEVICE_MISSING);

  await store.select(null);
  expect(state.selectedId).toBeNull();
  expect(store.deviceId).toBeNull();
});

test("failures map error codes to copy and keep the previous choice", async () => {
  const missing = new VoiceStore(
    createIpcClient(
      contract,
      createFakeBridge<Contract>().fail("voice.select", AppErrorCode.NOT_FOUND, "x"),
    ),
  );
  await missing.select("z");
  expect(missing.error).toBe(VOICE_SELECT_MISSING);
  expect(missing.deviceId).toBeNull();
  expect(missing.saving).toBe(false);

  const native = createFakeBridge<Contract>()
    .fail("voice.devices", AppErrorCode.NATIVE_ERROR, "x")
    .fail("voice.select", AppErrorCode.DB_ERROR, "x");
  const failed = new VoiceStore(createIpcClient(contract, native));
  await failed.refresh();
  expect(failed.error).toBe(VOICE_DEVICES_FAILED);
  expect(failed.loading).toBe(false);
  await failed.select("a");
  expect(failed.error).toBe(VOICE_SELECT_FAILED);
});

const MODEL: SpeechModelDto = {
  ref: "curated:speech:whisper-small-q8_0",
  displayName: "Whisper Small",
  description: "",
  sizeBytes: 264464607,
  state: "available",
  recommended: true,
};

function speechHost(models: VoiceModelsDto) {
  const state = { models };
  const bridge = createFakeBridge<Contract>()
    .handle("voice.models", () => state.models)
    .handle("voice.installModel", () => {
      state.models = {
        ...state.models,
        engineState: "downloading",
        models: state.models.models.map((model) => ({ ...model, state: "queued" })),
      };
      return state.models;
    })
    .handle("voice.startRecording", () => undefined)
    .handle("voice.stopRecording", () => ({ text: "Напиши письмо." }))
    .handle("voice.cancelRecording", () => undefined);
  return { store: new VoiceStore(createIpcClient(contract, bridge)), state, bridge };
}

const NOTHING: VoiceModelsDto = {
  models: [MODEL],
  selectedRef: null,
  engineState: "available",
  recommendedRef: MODEL.ref,
  reason: "16 ГБ оперативной памяти.",
};

test("installing polls the host until the downloads land and the model becomes ready", async () => {
  vi.useFakeTimers();
  try {
    const { store, state } = speechHost(NOTHING);
    await store.loadModels();
    expect(store.speechReady).toBe(false);
    expect(store.recommended?.ref).toBe(MODEL.ref);

    await store.installModel(MODEL.ref);
    expect(store.installPending).toBe(true);

    state.models = {
      ...NOTHING,
      engineState: "installed",
      selectedRef: MODEL.ref,
      models: [{ ...MODEL, state: "installed" }],
    };
    await vi.advanceTimersByTimeAsync(MODELS_POLL_MS);
    expect(store.installPending).toBe(false);
    expect(store.speechReady).toBe(true);
    store.dispose();
  } finally {
    vi.useRealTimers();
  }
});

test("a dictation moves through recording and recognition and returns its text", async () => {
  const { store, bridge } = speechHost(NOTHING);
  await store.startRecording();
  expect(store.phase).toBe("recording");
  const pending = store.stopRecording();
  expect(store.phase).toBe("recognizing");
  expect(await pending).toBe("Напиши письмо.");
  expect(store.phase).toBe("idle");

  await store.startRecording();
  await store.cancelRecording();
  expect(store.phase).toBe("idle");
  expect(bridge.calls.filter((call) => call.channel === "voice.cancelRecording")).toHaveLength(1);
});

test("recording failures map codes to copy and silence is reported", async () => {
  const unready = new VoiceStore(
    createIpcClient(
      contract,
      createFakeBridge<Contract>().fail("voice.startRecording", AppErrorCode.CONFLICT, "x"),
    ),
  );
  await unready.startRecording();
  expect(unready.recordError).toBe(VOICE_NOT_READY);
  expect(unready.phase).toBe("idle");

  const silent = new VoiceStore(
    createIpcClient(
      contract,
      createFakeBridge<Contract>()
        .handle("voice.startRecording", () => undefined)
        .handle("voice.stopRecording", () => ({ text: "" })),
    ),
  );
  await silent.startRecording();
  expect(await silent.stopRecording()).toBe("");
  expect(silent.recordError).toBe(VOICE_NOTHING_HEARD);

  const broken = new VoiceStore(
    createIpcClient(
      contract,
      createFakeBridge<Contract>()
        .handle("voice.startRecording", () => undefined)
        .fail("voice.stopRecording", AppErrorCode.NATIVE_ERROR, "x"),
    ),
  );
  await broken.startRecording();
  await broken.stopRecording();
  expect(broken.recordError).toBe(VOICE_RECOGNITION_FAILED);
});

test("dictated text is appended to the composer with a single separating space", () => {
  expect(appendDictation("", " Привет ", 100)).toBe("Привет");
  expect(appendDictation("Итак", "дальше", 100)).toBe("Итак дальше");
  expect(appendDictation("Итак\n", "дальше", 100)).toBe("Итак\nдальше");
  expect(appendDictation("abc", "   ", 100)).toBe("abc");
  expect(appendDictation("abc", "def", 5)).toBe("abc d");
});
