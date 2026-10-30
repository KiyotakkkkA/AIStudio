import { makeAutoObservable, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import type {
  CatalogueItemState,
  Contract,
  RecordingDeviceDto,
  SpeechModelDto,
  VoiceDevicesDto,
  VoiceModelsDto,
} from "@zvs/shared";
import {
  VOICE_DEVICE_MISSING,
  VOICE_DEVICES_FAILED,
  VOICE_MODEL_SELECT_FAILED,
  VOICE_MODELS_FAILED,
  VOICE_NOTHING_HEARD,
  VOICE_RECOGNITION_FAILED,
  installFailureCopy,
  startFailureCopy,
  systemDeviceLabel,
  voiceSelectFailureCopy,
} from "./voiceCopy";

export const MODELS_POLL_MS = 2000;

export type RecordingPhase = "idle" | "starting" | "recording" | "recognizing";

const PENDING: readonly CatalogueItemState[] = ["downloading", "queued"];

export default class VoiceStore {
  devices: RecordingDeviceDto[] = [];
  deviceId: string | null = null;
  loaded = false;
  loading = false;
  saving = false;
  error: string | null = null;

  models: SpeechModelDto[] = [];
  selectedModelRef: string | null = null;
  engineState: CatalogueItemState = "available";
  recommendedRef: string | null = null;
  reason = "";
  modelsLoaded = false;
  modelBusy = false;
  modelError: string | null = null;

  phase: RecordingPhase = "idle";
  recordError: string | null = null;
  private poll: ReturnType<typeof setTimeout> | undefined;

  constructor(private readonly ipc: IpcClient<Contract>) {
    makeAutoObservable<VoiceStore, "ipc" | "poll">(
      this,
      { ipc: false, poll: false },
      { autoBind: true },
    );
  }

  get selected(): RecordingDeviceDto | undefined {
    return this.devices.find((device) => device.id === this.deviceId);
  }

  get systemLabel(): string {
    return systemDeviceLabel(this.devices.find((device) => device.isDefault)?.name);
  }

  get selectedLabel(): string {
    if (this.deviceId === null) return this.systemLabel;
    return this.selected?.name ?? VOICE_DEVICE_MISSING;
  }

  get installedModels(): SpeechModelDto[] {
    return this.models.filter((model) => model.state === "installed");
  }

  get selectedModel(): SpeechModelDto | undefined {
    return this.installedModels.find((model) => model.ref === this.selectedModelRef);
  }

  get recommended(): SpeechModelDto | undefined {
    return this.models.find((model) => model.ref === this.recommendedRef);
  }

  get speechReady(): boolean {
    return this.engineState === "installed" && this.selectedModel !== undefined;
  }

  get installPending(): boolean {
    return (
      PENDING.includes(this.engineState) ||
      this.models.some((model) => PENDING.includes(model.state))
    );
  }

  async load(): Promise<void> {
    if (!this.loaded) await this.refresh();
    if (!this.modelsLoaded) await this.loadModels();
  }

  async refresh(): Promise<void> {
    if (this.loading) return;
    this.loading = true;
    this.error = null;
    try {
      this.apply(await this.ipc.call("voice.devices", undefined));
    } catch {
      runInAction(() => {
        this.error = VOICE_DEVICES_FAILED;
      });
    } finally {
      runInAction(() => {
        this.loading = false;
      });
    }
  }

  async select(id: string | null): Promise<void> {
    if (this.saving || this.deviceId === id) return;
    this.saving = true;
    this.error = null;
    try {
      this.apply(await this.ipc.call("voice.select", { deviceId: id }));
    } catch (error) {
      runInAction(() => {
        this.error = voiceSelectFailureCopy(error);
      });
    } finally {
      runInAction(() => {
        this.saving = false;
      });
    }
  }

  async loadModels(): Promise<void> {
    try {
      this.applyModels(await this.ipc.call("voice.models", undefined));
    } catch {
      runInAction(() => {
        this.modelError = VOICE_MODELS_FAILED;
      });
    }
  }

  async installModel(ref: string): Promise<void> {
    await this.changeModels(
      () => this.ipc.call("voice.installModel", { modelRef: ref }),
      installFailureCopy,
    );
  }

  async selectModel(ref: string | null): Promise<void> {
    if (this.selectedModelRef === ref) return;
    await this.changeModels(
      () => this.ipc.call("voice.selectModel", { modelRef: ref }),
      () => VOICE_MODEL_SELECT_FAILED,
    );
  }

  async startRecording(): Promise<void> {
    if (this.phase !== "idle") return;
    this.phase = "starting";
    this.recordError = null;
    try {
      await this.ipc.call("voice.startRecording", undefined);
      runInAction(() => {
        this.phase = "recording";
      });
    } catch (error) {
      runInAction(() => {
        this.phase = "idle";
        this.recordError = startFailureCopy(error);
      });
    }
  }

  async stopRecording(): Promise<string> {
    if (this.phase !== "recording") return "";
    this.phase = "recognizing";
    try {
      const { text } = await this.ipc.call("voice.stopRecording", undefined);
      runInAction(() => {
        if (text.trim().length === 0) this.recordError = VOICE_NOTHING_HEARD;
      });
      return text;
    } catch {
      runInAction(() => {
        this.recordError = VOICE_RECOGNITION_FAILED;
      });
      return "";
    } finally {
      runInAction(() => {
        this.phase = "idle";
      });
    }
  }

  async cancelRecording(): Promise<void> {
    if (this.phase !== "recording") return;
    this.phase = "idle";
    await this.ipc.call("voice.cancelRecording", undefined).catch(() => undefined);
  }

  dispose(): void {
    clearTimeout(this.poll);
    this.poll = undefined;
  }

  private async changeModels(
    action: () => Promise<VoiceModelsDto>,
    failure: (error: unknown) => string,
  ): Promise<void> {
    if (this.modelBusy) return;
    this.modelBusy = true;
    this.modelError = null;
    try {
      this.applyModels(await action());
    } catch (error) {
      runInAction(() => {
        this.modelError = failure(error);
      });
    } finally {
      runInAction(() => {
        this.modelBusy = false;
      });
    }
  }

  private apply(list: VoiceDevicesDto): void {
    this.devices = list.devices;
    this.deviceId = list.selectedId;
    this.loaded = true;
  }

  private applyModels(list: VoiceModelsDto): void {
    this.models = list.models;
    this.selectedModelRef = list.selectedRef;
    this.engineState = list.engineState;
    this.recommendedRef = list.recommendedRef;
    this.reason = list.reason;
    this.modelsLoaded = true;
    this.modelError = null;
    clearTimeout(this.poll);
    this.poll = this.installPending ? setTimeout(this.loadModels, MODELS_POLL_MS) : undefined;
  }
}
