import type { BrowserPendingDownload } from "@zvs/shared";

export const DOWNLOAD_KEEP = "Сохранить";
export const DOWNLOAD_DISCARD = "Отменить";
export const DOWNLOAD_LOADING = "Загрузка…";

const UNITS = ["Б", "КБ", "МБ", "ГБ"] as const;

export function downloadSize(bytes: number): string {
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const digits = unit === 0 || value >= 10 ? 0 : 1;
  return `${value.toLocaleString("ru-RU", { maximumFractionDigits: digits })} ${UNITS[unit]}`;
}

export function downloadTitle(download: BrowserPendingDownload): string {
  const site = download.host.length > 0 ? download.host : "Сайт";
  return `${site} загружает файл «${download.fileName}»`;
}

export function downloadDetail(download: BrowserPendingDownload, others: number): string {
  const size = download.ready ? downloadSize(download.bytes) : DOWNLOAD_LOADING;
  return others > 0 ? `${size} · ещё в очереди: ${others.toLocaleString("ru-RU")}` : size;
}
