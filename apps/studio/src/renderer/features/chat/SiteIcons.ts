import { makeAutoObservable, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import type { Contract, SiteIconDto, WebSourceDto } from "@zvs/shared";

export type CreateUrl = (blob: Blob) => string;

export function siteIconKey(source: WebSourceDto): string {
  try {
    return `${new URL(source.url).origin} ${source.icon ?? ""}`;
  } catch {
    return source.url;
  }
}

function iconBlob(icon: SiteIconDto): Blob {
  const binary = atob(icon.data);
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) bytes[index] = binary.charCodeAt(index);
  return new Blob([bytes], { type: icon.mime });
}

export default class SiteIcons {
  urls = new Map<string, string | null>();
  private readonly pending = new Set<string>();

  constructor(
    private readonly ipc: IpcClient<Contract>,
    private readonly createUrl: CreateUrl = (blob) => URL.createObjectURL(blob),
  ) {
    makeAutoObservable<SiteIcons, "ipc" | "pending" | "createUrl">(
      this,
      { ipc: false, pending: false, createUrl: false },
      { autoBind: true },
    );
  }

  url(source: WebSourceDto): string | null {
    return this.urls.get(siteIconKey(source)) ?? null;
  }

  async load(source: WebSourceDto) {
    const key = siteIconKey(source);
    if (this.urls.has(key) || this.pending.has(key)) return;
    this.pending.add(key);
    let url: string | null = null;
    try {
      const icon = await this.ipc.call("sites.icon", {
        url: source.url,
        ...(source.icon === undefined ? {} : { icon: source.icon }),
      });
      if (icon !== null) url = this.createUrl(iconBlob(icon));
    } catch {
      url = null;
    }
    runInAction(() => {
      this.pending.delete(key);
      this.urls.set(key, url);
    });
  }
}
