import { expect, test } from "vitest";
import { WebSourceDto } from "@zvs/shared";
import {
  CITE_PREFIX,
  WEB_PREFIX,
  citationNumber,
  linkCitations,
  linkWebSources,
  webSourceNumber,
} from "../../src/renderer/features/chat/citationMarkers.ts";
import {
  EFFORT_STEPS,
  FIXED_EFFORT_STEPS,
  effortStep,
  effortSteps,
  shownEffort,
  stepEffort,
} from "../../src/renderer/features/chat/accountSetupCopy.ts";
import {
  WEB_SOURCE_HOSTS_SHOWN,
  webSourceDetail,
  webSourceHost,
  webSourceHosts,
  webSourcesLabel,
} from "../../src/renderer/features/chat/webSources.ts";

const source = (url: string, date: string | null = null) =>
  WebSourceDto.parse({ url, title: url, date });

test("double-bracket markers link to the web sources they count, outside code fences", () => {
  const content = "Курс [[2]] и [[3]].\n```\n[[1]]\n```";
  expect(linkWebSources(content, 2)).toBe(
    `Курс [2](${WEB_PREFIX}2) и [[3]].\n\`\`\`\n[[1]]\n\`\`\``,
  );
  expect(linkWebSources(content, 0)).toBe(content);
  expect(webSourceNumber(`${WEB_PREFIX}2`)).toBe(2);
  expect(webSourceNumber(`${CITE_PREFIX}2`)).toBeNull();
  expect(citationNumber(`${WEB_PREFIX}2`)).toBeNull();
});

test("web markers survive the knowledge citation pass untouched", () => {
  const linked = linkCitations(linkWebSources("Итог [[1]] и [1].", 1), 1);
  expect(linked).toBe(`Итог [1](${WEB_PREFIX}1) и [1](${CITE_PREFIX}1).`);
});

test("sources read as hosts, dates and a counted label", () => {
  expect(webSourceHost("https://www.cbr.ru/currency/")).toBe("cbr.ru");
  expect(webSourceHost("not a url")).toBe("not a url");
  expect(webSourceDetail(source("https://rates.example/eur", "3 days ago"))).toBe(
    "rates.example · 3 days ago",
  );
  expect(webSourceDetail(source("https://rates.example/eur"))).toBe("rates.example");
  const hosts = webSourceHosts([
    source("https://a.example/1"),
    source("https://a.example/2"),
    source("https://b.example/"),
    source("https://c.example/"),
    source("https://d.example/"),
  ]);
  expect(hosts).toEqual(["a.example", "b.example", "c.example"]);
  expect(hosts).toHaveLength(WEB_SOURCE_HOSTS_SHOWN);
  expect([1, 3, 16].map(webSourcesLabel)).toEqual(["1 источник", "3 источника", "16 источников"]);
});

test("the effort slider snaps each position to one effort and back", () => {
  expect(EFFORT_STEPS.map((effort) => stepEffort(effortStep(effort)))).toEqual(EFFORT_STEPS);
  expect(stepEffort(0.4)).toBe("fast");
  expect(stepEffort(1.4)).toBe("auto");
  expect(stepEffort(9)).toBe("thinking");
  expect(stepEffort(-3)).toBe("fast");
});

test("models without adaptive reasoning offer only fast and thinking, showing auto as thinking", () => {
  expect(effortSteps(["streaming", "reasoning", "adaptive-reasoning"])).toBe(EFFORT_STEPS);
  expect(effortSteps(["streaming", "reasoning"])).toBe(FIXED_EFFORT_STEPS);
  expect(effortSteps(undefined)).toBe(EFFORT_STEPS);
  expect(shownEffort("auto", FIXED_EFFORT_STEPS)).toBe("thinking");
  expect(shownEffort("fast", FIXED_EFFORT_STEPS)).toBe("fast");
  expect(shownEffort("auto", EFFORT_STEPS)).toBe("auto");
  expect(effortStep("thinking", FIXED_EFFORT_STEPS)).toBe(1);
  expect(stepEffort(0.6, FIXED_EFFORT_STEPS)).toBe("thinking");
  expect(stepEffort(0.4, FIXED_EFFORT_STEPS)).toBe("fast");
});
