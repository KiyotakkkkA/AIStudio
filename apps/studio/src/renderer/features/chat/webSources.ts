import type { WebSourceDto } from "@zvs/shared";
import { plural } from "../connections/connectionPresentation";

export const WEB_SOURCES_TITLE = "Источники из сети";
export const COPY_LINK = "Копировать ссылку";
export const LINK_COPIED = "Ссылка скопирована";
export const WEB_SOURCE_HOSTS_SHOWN = 3;

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

export function webSourceHosts(sources: readonly WebSourceDto[]): string[] {
  return [...new Set(sources.map((source) => webSourceHost(source.url)))].slice(
    0,
    WEB_SOURCE_HOSTS_SHOWN,
  );
}

export function webSourceDetail(source: WebSourceDto): string {
  return [webSourceHost(source.url), source.date].filter(Boolean).join(" · ");
}
