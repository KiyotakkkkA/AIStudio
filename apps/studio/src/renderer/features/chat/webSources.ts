import type { WebSourceDto } from "@zvs/shared";
import { plural } from "../connections/connectionPresentation";

export const WEB_SOURCES_TITLE = "Источники из сети";
export const WEB_SOURCES_TAB = "Сеть";
export const COPY_LINK = "Копировать ссылку";
export const LINK_COPIED = "Ссылка скопирована";
export const OPEN_SITE = "Открыть во встроенном браузере";

export function webSourceHost(url: string): string {
  try {
    return new URL(url).hostname.replace(/^www\./, "");
  } catch {
    return url;
  }
}

export function webSourcesLabel(count: number): string {
  return plural(count, "источник", "источника", "источников");
}

export function webSourceDetail(source: WebSourceDto): string {
  return [webSourceHost(source.url), source.date].filter(Boolean).join(" · ");
}

export function siteInitial(source: WebSourceDto): string {
  const host = webSourceHost(source.url);
  const text = host.split(".").some((label) => label.startsWith("xn--")) ? source.title : host;
  return (text.match(/[\p{L}\p{N}]/u)?.[0] ?? "·").toUpperCase();
}
