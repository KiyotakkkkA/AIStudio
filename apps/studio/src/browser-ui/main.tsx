import { useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import type { BrowserCommand, BrowserCookie, BrowserState } from "@zvs/shared";
import {
  mdiArrowLeft,
  mdiArrowRight,
  mdiReload,
  mdiWeb,
  mdiPlus,
  mdiClose,
  mdiCookieOutline,
  mdiOpenInNew,
} from "@mdi/js";
import "../renderer/app/theme.css";
import "./style.css";

declare global {
  interface Window {
    zvs: {
      call(channel: string, payload: unknown): Promise<unknown>;
      subscribe(handler: (state: unknown) => void): () => void;
    };
  }
}

const securityLabels = {
  https: "HTTPS",
  http: "Подключение не защищено",
  none: "Соединение не обнаружено",
  error: "Ошибка загрузки контента",
};

function Icon({ path }: { path: string }) {
  return (
    <svg viewBox="0 0 24 24" width="16" height="16" fill="currentColor" aria-hidden="true">
      <path d={path} />
    </svg>
  );
}

function BrowserApp() {
  const [state, setState] = useState<BrowserState>({
    tabs: [],
    activeId: null,
    sitesVisible: false,
  });
  const [address, setAddress] = useState("");
  const [error, setError] = useState("");
  const [cookies, setCookies] = useState<BrowserCookie[]>([]);
  const [query, setQuery] = useState("");
  const [busy, setBusy] = useState(false);
  const [confirmation, setConfirmation] = useState<BrowserCommand | null>(null);
  const active = state.tabs.find((tab) => tab.id === state.activeId);

  async function send(command: BrowserCommand) {
    setError("");
    try {
      const result = await window.zvs.call("browser.command", command);
      if (Array.isArray(result)) setCookies(result as BrowserCookie[]);
      else setState(result as BrowserState);
    } catch {
      setError(
        "Action failed. Check the address (HTTP/HTTPS only) and try again. At most 30 tabs can be open.",
      );
    }
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

  const openSite = (url: string) => void send({ action: "new", url });
  const filtered = cookies.filter((cookie) =>
    `${cookie.domain} ${cookie.name}`.toLowerCase().includes(query.toLowerCase()),
  );
  const domains = [...new Set(filtered.map((cookie) => cookie.domain))].sort();

  return (
    <>
      <header>
        <div className="page-heading">
          <div className="page-title">
            <Icon path={mdiWeb} />
            <h1>Браузер</h1>
            <span className="heading-subtitle">Рабочее пространство</span>
          </div>
          <span className="profile-chip">
            <span className="profile-dot" />
            Work profile
          </span>
        </div>
        <div className="tabs" role="tablist" aria-label="Browser tabs">
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
                {tab.loading ? "◌ " : ""}
                {tab.title}
              </button>
              <button
                type="button"
                className="tab-close"
                aria-label={`Close ${tab.title}`}
                title="Close tab"
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
                <Icon path={mdiClose} />
              </button>
            </div>
          ))}
          <button aria-label="Новая вкладка" onClick={() => void send({ action: "new" })}>
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
          <button
            type="button"
            aria-label="Back"
            disabled={!active?.canGoBack}
            onClick={() => void send({ action: "back" })}
          >
            <Icon path={mdiArrowLeft} />
          </button>
          <button
            type="button"
            aria-label="Forward"
            disabled={!active?.canGoForward}
            onClick={() => void send({ action: "forward" })}
          >
            <Icon path={mdiArrowRight} />
          </button>
          <button type="button" aria-label="Reload" onClick={() => void send({ action: "reload" })}>
            <Icon path={mdiReload} />
          </button>
          <span
            className={`security ${active?.security ?? "none"}`}
            title="HTTPS indicates the loaded main page used HTTPS without an accepted certificate exception; it does not establish that a site is trustworthy."
          >
            {securityLabels[active?.security ?? "none"]}
          </span>
          <input
            aria-label="Address"
            placeholder="Enter a website address"
            value={address}
            onChange={(event) => setAddress(event.target.value)}
            onFocus={(event) => event.target.select()}
          />
          <button type="submit" className="primary">
            Перейти
          </button>
          <button
            type="button"
            aria-pressed={state.sitesVisible}
            onClick={() => void send({ action: "sites", visible: !state.sitesVisible })}
          >
            <Icon path={mdiCookieOutline} /> Сайты & cookies
          </button>
        </form>
      </header>
      <main>
        {state.sitesVisible ? (
          <section className="sites">
            <div className="heading">
              <div>
                <h1>Сайты & cookies</h1>
              </div>
              <button onClick={() => void send({ action: "sites", visible: false })}>Назад</button>
            </div>
            <div className="filters">
              <input
                aria-label="Filter cookies"
                placeholder="Поиск по домену или имени..."
                value={query}
                onChange={(event) => setQuery(event.target.value)}
              />
              <button onClick={() => void send({ action: "cookies" })}>Обновить</button>
              <span>{cookies.length} cookies</span>
            </div>
            {domains.length === 0 && (
              <p className="empty">
                Нет куки. Совершайте действия на сайтах чтобы заполнить этот список.
              </p>
            )}
            {domains.map((domain) => (
              <article key={domain}>
                <div className="heading">
                  <h2>{domain}</h2>
                  <button
                    disabled={busy}
                    onClick={() => setConfirmation({ action: "clearSite", domain })}
                  >
                    Очистить cookies
                  </button>
                </div>
                <table>
                  <thead>
                    <tr>
                      <th>Имя / путь</th>
                      <th>Флаги</th>
                      <th>Истекает</th>
                      <th>Действия</th>
                    </tr>
                  </thead>
                  <tbody>
                    {filtered
                      .filter((cookie) => cookie.domain === domain)
                      .map((cookie) => (
                        <tr key={cookie.id}>
                          <td>
                            {cookie.name}
                            <small>{cookie.path}</small>
                          </td>
                          <td>
                            {[cookie.secure && "Secure", cookie.httpOnly && "HttpOnly"]
                              .filter(Boolean)
                              .join(" · ") || "—"}
                          </td>
                          <td>
                            {cookie.session
                              ? "Session"
                              : cookie.expirationDate
                                ? new Date(cookie.expirationDate * 1000).toLocaleString()
                                : "—"}
                          </td>
                          <td>
                            <button
                              disabled={busy}
                              onClick={() =>
                                setConfirmation({ action: "removeCookie", id: cookie.id })
                              }
                            >
                              Удалить
                            </button>
                          </td>
                        </tr>
                      ))}
                  </tbody>
                </table>
              </article>
            ))}
            {confirmation && (
              <div className="overlay">
                <div role="dialog" aria-modal="true" aria-labelledby="confirm-title">
                  <h2 id="confirm-title">
                    Удалить{" "}
                    {confirmation.action === "clearSite"
                      ? `cookies for ${confirmation.domain}`
                      : "this cookie"}
                    ?
                  </h2>
                  <p>This may sign you out. You can log in again on the site.</p>
                  <button disabled={busy} onClick={() => setConfirmation(null)}>
                    Cancel
                  </button>{" "}
                  <button
                    disabled={busy}
                    onClick={() => {
                      setBusy(true);
                      void send(confirmation).finally(() => {
                        setBusy(false);
                        setConfirmation(null);
                      });
                    }}
                  >
                    Confirm removal
                  </button>
                </div>
              </div>
            )}
          </section>
        ) : (
          !active?.url && (
            <section className="welcome">
              <span className="badge">WORK PROFILE</span>
              <div className="welcome-icon">
                <Icon path={mdiWeb} />
              </div>
              <h1>Ваш браузер - ваше пространство</h1>
              <div className="shortcuts">
                <button className="site-card" onClick={() => openSite("https://chat.qwen.ai/")}>
                  <span className="site-avatar">Q</span>
                  <span>
                    <strong>Qwen</strong>
                    <small>chat.qwen.ai</small>
                  </span>
                  <Icon path={mdiOpenInNew} />
                </button>
                <button
                  className="site-card"
                  onClick={() => openSite("https://chat.deepseek.com/")}
                >
                  <span className="site-avatar">D</span>
                  <span>
                    <strong>DeepSeek</strong>
                    <small>chat.deepseek.com</small>
                  </span>
                  <Icon path={mdiOpenInNew} />
                </button>
              </div>
            </section>
          )
        )}
      </main>
      <footer>
        <span role="status">
          {error || active?.error || (active?.loading ? "Loading…" : "Ready")}
        </span>
        <span>Profile: work · Downloads blocked</span>
      </footer>
    </>
  );
}

createRoot(document.getElementById("root")!).render(<BrowserApp />);
