import type { Accelerator, DeviceProfileDto, ModelFormat, RuntimeEngine } from "@zvs/shared";

/**
 * One installable engine build. A definition is platform-independent; `assets` decides whether
 * a build exists for the machine we are on, and returns `undefined` when it does not — which is
 * how a CUDA row renders as `unsupported` on a Mac rather than being silently missing.
 */
export interface RuntimeDefinition {
  readonly id: string;
  readonly engine: RuntimeEngine;
  readonly accelerator: Accelerator;
  readonly displayName: string;
  readonly description: string;
  readonly formats: readonly ModelFormat[];
  readonly approximateBytes: number;
  assets(platform: NodeJS.Platform, arch: string): AssetPlan | undefined;
}

export interface AssetPlan {
  readonly primary: RegExp;
  /**
   * Matched against the same release and pinned to the primary's variant version. llama.cpp
   * ships the CUDA runtime DLLs separately, and a CUDA build without them will not start on a
   * machine that has no CUDA toolkit installed — which is most machines.
   */
  readonly companion?: (variant: string) => RegExp;
}

const WINDOWS_ARCH: Record<string, string | undefined> = { x64: "x64", arm64: "arm64" };
const UNIX_ARCH: Record<string, string | undefined> = { x64: "x64", arm64: "arm64" };

function llamaAsset(platform: NodeJS.Platform, arch: string, backend: string): RegExp | undefined {
  if (platform === "win32") {
    const suffix = WINDOWS_ARCH[arch];
    return suffix === undefined
      ? undefined
      : new RegExp(`^llama-b\\d+-bin-win-${backend}-${suffix}\\.zip$`);
  }
  if (platform === "linux") {
    const suffix = UNIX_ARCH[arch];
    return suffix === undefined
      ? undefined
      : new RegExp(`^llama-b\\d+-bin-ubuntu-${backend}-${suffix}\\.tar\\.gz$`);
  }
  return undefined;
}

/**
 * The builds we offer. Vulkan is deliberately the GPU default rather than CUDA: it is 30 MB
 * instead of 400 MB, needs no toolkit, and accelerates NVIDIA, AMD and Intel alike. CUDA stays
 * available for the NVIDIA owner who wants the last 20 % of throughput.
 */
export const RUNTIME_DEFINITIONS: readonly RuntimeDefinition[] = [
  {
    id: "llama-cpp-cpu",
    engine: "llama-cpp",
    accelerator: "cpu",
    displayName: "llama.cpp · CPU",
    description:
      "Движок для моделей в формате GGUF, считает на процессоре. Работает на любой машине, но эмбеддинги большого корпуса займут заметно больше времени.",
    formats: ["gguf"],
    approximateBytes: 19_000_000,
    assets(platform, arch) {
      if (platform === "darwin") {
        const suffix = UNIX_ARCH[arch];
        return suffix === undefined
          ? undefined
          : { primary: new RegExp(`^llama-b\\d+-bin-macos-${suffix}\\.tar\\.gz$`) };
      }
      if (platform === "linux") {
        const suffix = UNIX_ARCH[arch];
        return suffix === undefined
          ? undefined
          : { primary: new RegExp(`^llama-b\\d+-bin-ubuntu-${suffix}\\.tar\\.gz$`) };
      }
      const primary = llamaAsset(platform, arch, "cpu");
      return primary === undefined ? undefined : { primary };
    },
  },
  {
    id: "llama-cpp-vulkan",
    engine: "llama-cpp",
    accelerator: "vulkan",
    displayName: "llama.cpp · Vulkan (GPU)",
    description:
      "Тот же движок с расчётом на видеокарте через Vulkan. Подходит NVIDIA, AMD и встроенной графике Intel, не требует установки CUDA и весит около 30 МБ. Рекомендуемый вариант для любой дискретной видеокарты.",
    formats: ["gguf"],
    approximateBytes: 32_000_000,
    assets(platform, arch) {
      const primary = llamaAsset(platform, arch, "vulkan");
      return primary === undefined ? undefined : { primary };
    },
  },
  {
    id: "llama-cpp-cuda",
    engine: "llama-cpp",
    accelerator: "cuda",
    displayName: "llama.cpp · CUDA (NVIDIA)",
    description:
      "Сборка под CUDA: самая быстрая на видеокартах NVIDIA. Скачивается вместе с библиотеками CUDA, поэтому занимает несколько сотен мегабайт. Имеет смысл, только если карта — NVIDIA.",
    formats: ["gguf"],
    approximateBytes: 545_000_000,
    assets(platform, arch) {
      if (platform === "win32" && WINDOWS_ARCH[arch] !== undefined) {
        const suffix = WINDOWS_ARCH[arch];
        return {
          primary: new RegExp(`^llama-b\\d+-bin-win-cuda-([\\d.]+)-${suffix}\\.zip$`),
          companion: (variant) =>
            new RegExp(`^cudart-llama-bin-win-cuda-${escape(variant)}-${suffix}\\.zip$`),
        };
      }
      if (platform === "linux" && UNIX_ARCH[arch] !== undefined) {
        const suffix = UNIX_ARCH[arch];
        return {
          primary: new RegExp(`^llama-b\\d+-bin-ubuntu-cuda-([\\d.]+)-${suffix}\\.tar\\.gz$`),
          companion: (variant) =>
            new RegExp(
              `^cudart-llama-b\\d+-bin-ubuntu-cuda-${escape(variant)}-${suffix}\\.tar\\.gz$`,
            ),
        };
      }
      return undefined;
    },
  },
  {
    id: "llama-cpp-metal",
    engine: "llama-cpp",
    accelerator: "metal",
    displayName: "llama.cpp · Metal (Apple)",
    description:
      "Сборка для Apple Silicon: расчёт идёт на встроенном графическом ядре через Metal. Отдельно ничего ставить не нужно.",
    formats: ["gguf"],
    approximateBytes: 12_000_000,
    assets(platform, arch) {
      if (platform !== "darwin" || arch !== "arm64") return undefined;
      return { primary: /^llama-b\d+-bin-macos-arm64\.tar\.gz$/ };
    },
  },
];

function escape(value: string): string {
  return value.replaceAll(".", "\\.");
}

export function definitionOf(id: string): RuntimeDefinition | undefined {
  return RUNTIME_DEFINITIONS.find((definition) => definition.id === id);
}

export interface AcceleratorPlan {
  readonly accelerator: Accelerator;
  readonly reason: string;
}

/**
 * Which build this machine should install. The reason is user-facing and names what was
 * detected, so someone who disagrees with the verdict can see why it was reached.
 */
export function planAccelerator(
  device: DeviceProfileDto | null,
  platform: NodeJS.Platform = process.platform,
  arch: string = process.arch,
): AcceleratorPlan {
  if (platform === "darwin" && arch === "arm64")
    return { accelerator: "metal", reason: "Apple Silicon — расчёт идёт через Metal." };
  const gpu = device?.gpu ?? null;
  if (gpu === null)
    return {
      accelerator: "cpu",
      reason: "Видеокарту определить не удалось, поэтому предлагается сборка для процессора.",
    };
  if (!gpu.discrete)
    return {
      accelerator: "cpu",
      reason: `Обнаружена встроенная графика (${gpu.vendor}), ускорение даст немного — предлагается сборка для процессора.`,
    };
  return {
    accelerator: "vulkan",
    reason: `Обнаружена дискретная видеокарта ${gpu.vendor} ${gpu.model}. Vulkan ускоряет расчёт и не требует CUDA.`,
  };
}

export function formatOf(fileName: string): ModelFormat | undefined {
  const lower = fileName.toLowerCase();
  if (lower.endsWith(".gguf")) return "gguf";
  if (lower.endsWith(".onnx")) return "onnx";
  return undefined;
}
