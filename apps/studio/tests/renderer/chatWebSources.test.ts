import { expect, test, vi } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import { WebSourceDto, contract, type Contract } from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge.ts";
import { createEventRouter } from "../../src/renderer/app/EventRouter.ts";
import ChatStore from "../../src/renderer/features/chat/ChatStore.ts";
import SiteIcons, { siteIconKey } from "../../src/renderer/features/chat/SiteIcons.ts";
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
  siteInitial,
  webSourceDetail,
  webSourceHost,
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
  expect([1, 3, 16].map(webSourcesLabel)).toEqual(["1 источник", "3 источника", "16 источников"]);
  expect(siteInitial(source("https://www.cbr.ru/"))).toBe("C");
  expect(siteInitial(WebSourceDto.parse({ url: "https://яндекс.рф/", title: "яндекс" }))).toBe("Я");
});

test("a site icon is asked for once per site and kept as a blob address", async () => {
  const bridge = createFakeBridge<Contract>().handle("sites.icon", (input) =>
    input.url.startsWith("https://a.example") ? { mime: "image/png", data: btoa("\x89PNG") } : null,
  );
  const created: Blob[] = [];
  const icons = new SiteIcons(createIpcClient(contract, bridge), (blob) => {
    created.push(blob);
    return `blob:icon-${String(created.length)}`;
  });
  const first = source("https://a.example/news/1");
  const sibling = source("https://a.example/news/2");
  const vendor = WebSourceDto.parse({
    url: "https://a.example/news/3",
    title: "x",
    icon: "https://cdn.example/a.png",
  });
  expect(icons.url(first)).toBeNull();
  await Promise.all([icons.load(first), icons.load(sibling)]);
  expect(icons.url(sibling)).toBe("blob:icon-1");
  expect(created[0]?.type).toBe("image/png");
  await icons.load(vendor);
  await icons.load(source("https://b.example/"));
  expect(icons.url(source("https://b.example/x"))).toBeNull();
  const asked = bridge.calls.filter((call) => call.channel === "sites.icon");
  expect(asked.map((call) => call.payload)).toEqual([
    { url: "https://a.example/news/1" },
    { url: "https://a.example/news/3", icon: "https://cdn.example/a.png" },
    { url: "https://b.example/" },
  ]);
  expect(siteIconKey(first)).toBe(siteIconKey(sibling));
  expect(siteIconKey(vendor)).not.toBe(siteIconKey(first));
});

test("a source opens in the integrated browser and a failure shows as an error", async () => {
  let fail = false;
  const bridge = createFakeBridge<Contract>().handle("sites.open", () => {
    if (fail) throw new Error("boom");
    return undefined;
  });
  const events = createEventRouter({ logger: { log: vi.fn() } });
  const store = new ChatStore(createIpcClient(contract, bridge), events);
  await store.openSite("https://a.example/news");
  expect(bridge.calls.at(-1)).toMatchObject({
    channel: "sites.open",
    payload: { url: "https://a.example/news" },
  });
  expect(store.error).toBeNull();
  fail = true;
  await store.openSite("https://a.example/news");
  expect(store.error).not.toBeNull();
  events.dispose();
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
