import "./tauriBridge";
import { useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import type { BrowserCommand, BrowserCookie, BrowserState } from "@zvs/shared";
import {
  mdiAlertCircleOutline,
  mdiArrowLeft,
  mdiArrowRight,
  mdiArrowRightCircleOutline,
  mdiCodeTags,
  mdiCookieOutline,
  mdiClose,
  mdiDeleteSweepOutline,
  mdiDownloadOffOutline,
  mdiLock,
  mdiLockOpenVariantOutline,
  mdiMagnify,
  mdiPlus,
  mdiRefresh,
  mdiReload,
  mdiRobotOutline,
  mdiTrashCanOutline,
  mdiWeb,
} from "@mdi/js";
import "../renderer/app/theme.css";
import "./style.css";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import Icon from "./Icon";
import AgentPanel from "./AgentPanel";
import ApprovalBar from "./ApprovalBar";
import IconButton from "./IconButton";
import NetworkPanel from "./NetworkPanel";
import {
  AGENT_ACTION_FAILED,
  AGENT_CLOSE,
  AGENT_OPEN,
  STATUS_LINES,
  STATUS_TONES,
} from "./agentCopy";
import { DEVTOOLS_CLOSE, DEVTOOLS_OPEN } from "./networkCopy";
import {
  ACTION_FAILED,
  CANCEL,
  CLEAR_SITE,
  CONFIRM_REMOVAL,
  PROFILE_FOOTER,
  PROFILE_LABEL,
  REMOVE_COOKIE,
  SECURITY_HINT,
  SECURITY_LABELS,
  SESSION_COOKIE,
  SITES_EMPTY,
  SITES_EMPTY_HINT,
  SITES_HINT,
  SITES_NOTHING_FOUND,
  SITES_TITLE,
  STATUS_LOADING,
  STATUS_READY,
  confirmTitle,
  cookiesWord,
  expiryLabel,
  sitesWord,
} from "./browserCopy";

declare global {
  interface Window {
    zvs: {
      call(channel: string, payload: unknown): Promise<unknown>;
      subscribe(handler: (state: unknown) => void): () => void;
    };
  }
}

const SECURITY_ICONS = {
  https: mdiLock,
  http: mdiLockOpenVariantOutline,
  none: mdiWeb,
  error: mdiAlertCircleOutline,
} as const;

function siteInitial(domain: string): string {
  return domain.replace(/^\./, "").charAt(0).toUpperCase() || "·";
}

function BrowserApp() {
  const [state, setState] = useState<BrowserState>({
    tabs: [],
    activeId: null,
    sitesVisible: false,
    devtoolsVisible: false,
    agentVisible: false,
    agent: {
      status: "idle",
      runId: null,
      task: null,
      steps: 0,
      pace: 0,
      limit: 0,
      target: null,
      approval: null,
      actions: [],
      origin: null,
      grants: [],
    },
  });
  const [address, setAddress] = useState("");
  const [error, setError] = useState("");
  const [cookies, setCookies] = useState<BrowserCookie[]>([]);
  const [query, setQuery] = useState("");
  const [busy, setBusy] = useState(false);
  const [agentBusy, setAgentBusy] = useState(false);
  const [confirmation, setConfirmation] = useState<BrowserCommand | null>(null);
  const active = state.tabs.find((tab) => tab.id === state.activeId);
  const security = active?.security ?? "none";

  async function send(command: BrowserCommand) {
    setError("");
    try {
      const result = await window.zvs.call("browser.command", command);
      if (Array.isArray(result)) setCookies(result as BrowserCookie[]);
      else setState(result as BrowserState);
    } catch {
      setError(command.action.startsWith("agent") ? AGENT_ACTION_FAILED : ACTION_FAILED);
    }
  }

  function sendAgent(command: BrowserCommand) {
    setAgentBusy(true);
    void send(command).finally(() => setAgentBusy(false));
  }

  useEffect(() => {
    const unsubscribe = window.zvs.subscribe((value) => setState(value as BrowserState));
    void send({ action: "state" });
    return unsubscribe;
  }, []);
  useEffect(() => setAddress(active?.url ?? ""), [active?.url, active?.id]);
  useEffect(() => {
    if (!state.sitesVisible) return;
    void send({ action: "cookies" });
    const timer = window.setInterval(() => void send({ action: "cookies" }), 5000);
    return () => window.clearInterval(timer);
  }, [state.sitesVisible]);

  const filtered = cookies.filter((cookie) =>
    `${cookie.domain} ${cookie.name}`.toLowerCase().includes(query.toLowerCase()),
  );
  const domains = [...new Set(filtered.map((cookie) => cookie.domain))].sort();
  const allDomains = new Set(cookies.map((cookie) => cookie.domain)).size;
  const agent = state.agent;
  const agentActive = agent.status !== "idle";
  const statusText =
    error ||
    active?.error ||
    (agentActive ? STATUS_LINES[agent.status] : active?.loading ? STATUS_LOADING : STATUS_READY);
  const statusTone =
    error || active?.error
      ? "err"
      : agentActive
        ? STATUS_TONES[agent.status]
        : active?.loading
          ? "busy"
          : "ok";

  return (
    <>
      <header>
        <div className="page-heading">
          <div className="page-title">
            <span className="title-mark">
              <Icon path={mdiWeb} size={18} />
            </span>
            <div className="page-title-text">
              <h1>Браузер</h1>
              <span className="heading-subtitle">Рабочее пространство</span>
            </div>
          </div>
          <span className="profile-chip">
            <span className="profile-dot" />
            {PROFILE_LABEL}
          </span>
        </div>
        <div className="tabs" role="tablist" aria-label="Вкладки браузера">
          {state.tabs.map((tab, index) => (
            <div
              className={`tab ${tab.id === state.activeId ? "active" : ""}`}
              key={tab.id}
              draggable
              onDragStart={(event) => event.dataTransfer.setData("text/plain", tab.id)}
              onDragOver={(event) => event.preventDefault()}
              onDrop={(event) => {
                event.preventDefault();
                void send({
                  action: "reorder",
                  id: event.dataTransfer.getData("text/plain"),
                  index,
                });
              }}
            >
              <span className={`tab-glyph ${tab.loading ? "loading" : ""}`} aria-hidden="true">
                {tab.loading ? <span className="spinner" /> : <Icon path={mdiWeb} size={13} />}
              </span>
              <button
                type="button"
                className="tab-title"
                role="tab"
                aria-selected={tab.id === state.activeId}
                title={tab.title}
                onClick={() => void send({ action: "switch", id: tab.id })}
                onKeyDown={(event) => {
                  if (event.altKey && ["ArrowLeft", "ArrowRight"].includes(event.key)) {
                    event.preventDefault();
                    void send({
                      action: "reorder",
                      id: tab.id,
                      index: Math.max(
                        0,
                        Math.min(
                          state.tabs.length - 1,
                          index + (event.key === "ArrowLeft" ? -1 : 1),
                        ),
                      ),
                    });
                  }
                }}
              >
                {tab.title}
              </button>
              <button
                type="button"
                className="tab-close"
                aria-label={`Закрыть ${tab.title}`}
                title="Закрыть вкладку"
                draggable={false}
                onPointerDown={(event) => event.stopPropagation()}
                onDragStart={(event) => {
                  event.preventDefault();
                  event.stopPropagation();
                }}
                onClick={(event) => {
                  event.preventDefault();
                  event.stopPropagation();
                  void send({ action: "close", id: tab.id });
                }}
              >
                <Icon path={mdiClose} size={14} />
              </button>
            </div>
          ))}
          <button
            type="button"
            className="new-tab"
            aria-label="Новая вкладка"
            title="Новая вкладка"
            onClick={() => void send({ action: "new" })}
          >
            <Icon path={mdiPlus} />
          </button>
        </div>
        <form
          className="toolbar"
          onSubmit={(event) => {
            event.preventDefault();
            void send({ action: "navigate", url: address });
          }}
        >
          <div className="nav-group">
            <IconButton
              path={mdiArrowLeft}
              label="Назад"
              disabled={!active?.canGoBack}
              onClick={() => void send({ action: "back" })}
            />
            <IconButton
              path={mdiArrowRight}
              label="Вперед"
              disabled={!active?.canGoForward}
              onClick={() => void send({ action: "forward" })}
            />
            <IconButton
              path={mdiReload}
              label="Обновить страницу"
              onClick={() => void send({ action: "reload" })}
            />
          </div>
          <label className="omnibox">
            <span className={`security ${security}`} title={SECURITY_HINT}>
              <Icon path={SECURITY_ICONS[security]} size={13} />
              <span className="security-label">{SECURITY_LABELS[security]}</span>
            </span>
            <input
              aria-label="Адрес"
              placeholder="Введите адрес сайта"
              value={address}
              spellCheck={false}
              onChange={(event) => setAddress(event.target.value)}
              onFocus={(event) => event.target.select()}
            />
          </label>
          <div className="nav-group">
            <IconButton type="submit" path={mdiArrowRightCircleOutline} label="Перейти" />
            <IconButton
              path={mdiCookieOutline}
              label={SITES_TITLE}
              className="toggle"
              aria-pressed={state.sitesVisible}
              onClick={() => void send({ action: "sites", visible: !state.sitesVisible })}
            />
            <IconButton
              path={mdiRobotOutline}
              label={state.agentVisible ? AGENT_CLOSE : AGENT_OPEN}
              className="toggle"
              aria-pressed={state.agentVisible}
              onClick={() => void send({ action: "agent", visible: !state.agentVisible })}
            />
            <IconButton
              tooltip="top-right"
              path={mdiCodeTags}
              label={state.devtoolsVisible ? DEVTOOLS_CLOSE : DEVTOOLS_OPEN}
              className="toggle"
              aria-pressed={state.devtoolsVisible}
              onClick={() => void send({ action: "devtools", visible: !state.devtoolsVisible })}
            />
          </div>
        </form>
      </header>
      <main>
        <div className="page-column">
          <ScrollArea orientation="vertical" className="page-area">
            {state.sitesVisible ? (
              <section className="sites">
                <div className="sites-heading">
                  <span className="title-mark">
                    <Icon path={mdiCookieOutline} size={18} />
                  </span>
                  <div className="sites-heading-text">
                    <h1>{SITES_TITLE}</h1>
                    <p>{SITES_HINT}</p>
                  </div>
                  <button
                    type="button"
                    onClick={() => void send({ action: "sites", visible: false })}
                  >
                    <Icon path={mdiArrowLeft} size={15} />
                    Назад
                  </button>
                </div>
                <div className="filters">
                  <label className="search">
                    <Icon path={mdiMagnify} />
                    <input
                      aria-label="Поиск куки"
                      placeholder="Поиск по домену или имени"
                      value={query}
                      spellCheck={false}
                      onChange={(event) => setQuery(event.target.value)}
                    />
                  </label>
                  <span className="stat">{sitesWord(allDomains)}</span>
                  <span className="stat">{cookiesWord(cookies.length)}</span>
                  <button type="button" onClick={() => void send({ action: "cookies" })}>
                    <Icon path={mdiRefresh} size={15} />
                    Обновить
                  </button>
                </div>
                {domains.length === 0 && (
                  <div className="empty">
                    <span className="empty-mark">
                      <Icon path={mdiCookieOutline} size={22} />
                    </span>
                    <strong>{cookies.length === 0 ? SITES_EMPTY : SITES_NOTHING_FOUND}</strong>
                    {cookies.length === 0 ? <p>{SITES_EMPTY_HINT}</p> : null}
                  </div>
                )}
                {domains.map((domain) => {
                  const rows = filtered.filter((cookie) => cookie.domain === domain);
                  return (
                    <article key={domain}>
                      <div className="site-heading">
                        <span className="site-avatar" aria-hidden="true">
                          {siteInitial(domain)}
                        </span>
                        <h2>{domain}</h2>
                        <span className="count">{cookiesWord(rows.length)}</span>
                        <button
                          type="button"
                          className="danger-soft"
                          disabled={busy}
                          onClick={() => setConfirmation({ action: "clearSite", domain })}
                        >
                          <Icon path={mdiDeleteSweepOutline} size={15} />
                          {CLEAR_SITE}
                        </button>
                      </div>
                      <table>
                        <thead>
                          <tr>
                            <th>Имя / путь</th>
                            <th>Флаги</th>
                            <th>Истекает</th>
                            <th className="actions">
                              <span className="sr-only">Действия</span>
                            </th>
                          </tr>
                        </thead>
                        <tbody>
                          {rows.map((cookie) => (
                            <tr key={cookie.id}>
                              <td className="cookie-name">
                                {cookie.name}
                                <small>{cookie.path}</small>
                              </td>
                              <td>
                                <span className="flags">
                                  {cookie.secure ? <span className="flag ok">Secure</span> : null}
                                  {cookie.httpOnly ? (
                                    <span className="flag info">HttpOnly</span>
                                  ) : null}
                                  {!cookie.secure && !cookie.httpOnly ? (
                                    <span className="muted">—</span>
                                  ) : null}
                                </span>
                              </td>
                              <td className="expiry">
                                {cookie.session ? (
                                  <span className="flag neutral">{SESSION_COOKIE}</span>
                                ) : cookie.expirationDate ? (
                                  expiryLabel(cookie.expirationDate)
                                ) : (
                                  <span className="muted">—</span>
                                )}
                              </td>
                              <td className="actions">
                                <button
                                  type="button"
                                  className="row-action"
                                  aria-label={REMOVE_COOKIE}
                                  title={REMOVE_COOKIE}
                                  disabled={busy}
                                  onClick={() =>
                                    setConfirmation({ action: "removeCookie", id: cookie.id })
                                  }
                                >
                                  <Icon path={mdiTrashCanOutline} size={15} />
                                </button>
                              </td>
                            </tr>
                          ))}
                        </tbody>
                      </table>
                    </article>
                  );
                })}
                {confirmation && (
                  <div className="overlay">
                    <div role="dialog" aria-modal="true" aria-labelledby="confirm-title">
                      <span className="dialog-mark">
                        <Icon path={mdiTrashCanOutline} size={18} />
                      </span>
                      <h2 id="confirm-title">
                        {confirmTitle(
                          confirmation.action === "clearSite" ? confirmation.domain : null,
                        )}
                      </h2>
                      <p>Сеанс на сайте может завершиться; войти можно снова.</p>
                      <div className="dialog-actions">
                        <button type="button" disabled={busy} onClick={() => setConfirmation(null)}>
                          {CANCEL}
                        </button>
                        <button
                          type="button"
                          className="danger"
                          disabled={busy}
                          onClick={() => {
                            setBusy(true);
                            void send(confirmation).finally(() => {
                              setBusy(false);
                              setConfirmation(null);
                            });
                          }}
                        >
                          {CONFIRM_REMOVAL}
                        </button>
                      </div>
                    </div>
                  </div>
                )}
              </section>
            ) : (
              !active?.url && (
                <section className="welcome">
                  <span className="badge">{PROFILE_LABEL}</span>
                  <div className="welcome-icon">
                    <Icon path={mdiWeb} size={28} />
                  </div>
                  <h1>Новая вкладка</h1>
                  <p>
                    Введите адрес в строке выше. Вход на сайтах и куки сохраняются в рабочем
                    профиле.
                  </p>
                </section>
              )
            )}
          </ScrollArea>
          {agent.approval && (
            <ApprovalBar approval={agent.approval} busy={agentBusy} send={sendAgent} />
          )}
          {state.devtoolsVisible && (
            <NetworkPanel
              tabId={state.activeId}
              onClose={() => void send({ action: "devtools", visible: false })}
            />
          )}
        </div>
        {state.agentVisible && <AgentPanel agent={agent} busy={agentBusy} send={sendAgent} />}
      </main>
      <footer>
        <span role="status" className={`status ${statusTone}`}>
          <span className="status-dot" aria-hidden="true" />
          {statusText}
        </span>
        <span className="profile-line">
          <Icon path={mdiDownloadOffOutline} size={12} />
          {PROFILE_FOOTER}
        </span>
      </footer>
    </>
  );
}

createRoot(document.getElementById("root")!).render(<BrowserApp />);
