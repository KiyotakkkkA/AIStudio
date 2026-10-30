import { z } from "zod";
import { CatalogueItemState, ItemRef } from "./downloads/DownloadDto.js";

export const RECORDING_DEVICE_ID_MAX = 512;
export const RECORDING_DEVICE_NAME_MAX = 200;
export const SPEECH_MODEL_NAME_MAX = 200;
export const SPEECH_MODEL_DESCRIPTION_MAX = 1000;
export const SPEECH_REASON_MAX = 500;
export const VOICE_TRANSCRIPT_MAX = 100_000;

export const RecordingDeviceId = z.string().min(1).max(RECORDING_DEVICE_ID_MAX);

export const RecordingDeviceDto = z.object({
  id: RecordingDeviceId,
  name: z.string().min(1).max(RECORDING_DEVICE_NAME_MAX),
  isDefault: z.boolean(),
});
export type RecordingDeviceDto = z.infer<typeof RecordingDeviceDto>;

export const VoiceDevicesDto = z
  .object({ devices: z.array(RecordingDeviceDto), selectedId: RecordingDeviceId.nullable() })
  .refine((list) => list.devices.filter((device) => device.isDefault).length <= 1, {
    path: ["devices"],
  });
export type VoiceDevicesDto = z.infer<typeof VoiceDevicesDto>;

export const VoiceSelectInput = z.object({ deviceId: RecordingDeviceId.nullable() });
export type VoiceSelectInput = z.infer<typeof VoiceSelectInput>;

export const SpeechModelDto = z.object({
  ref: ItemRef,
  displayName: z.string().min(1).max(SPEECH_MODEL_NAME_MAX),
  description: z.string().max(SPEECH_MODEL_DESCRIPTION_MAX),
  sizeBytes: z.number().int().nonnegative().max(Number.MAX_SAFE_INTEGER),
  state: CatalogueItemState,
  recommended: z.boolean(),
});
export type SpeechModelDto = z.infer<typeof SpeechModelDto>;

export const VoiceModelsDto = z
  .object({
    models: z.array(SpeechModelDto),
    selectedRef: ItemRef.nullable(),
    engineState: CatalogueItemState,
    recommendedRef: ItemRef,
    reason: z.string().max(SPEECH_REASON_MAX),
  })
  .refine((list) => list.models.filter((model) => model.recommended).length <= 1, {
    path: ["models"],
  });
export type VoiceModelsDto = z.infer<typeof VoiceModelsDto>;

export const VoiceModelInput = z.object({ modelRef: ItemRef });
export type VoiceModelInput = z.infer<typeof VoiceModelInput>;

export const VoiceSelectModelInput = z.object({ modelRef: ItemRef.nullable() });
export type VoiceSelectModelInput = z.infer<typeof VoiceSelectModelInput>;

export const VoiceTranscriptDto = z.object({ text: z.string().max(VOICE_TRANSCRIPT_MAX) });
export type VoiceTranscriptDto = z.infer<typeof VoiceTranscriptDto>;
