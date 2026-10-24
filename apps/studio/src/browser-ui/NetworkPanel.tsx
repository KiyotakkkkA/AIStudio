import { useEffect, useState } from "react";
import type {
  BrowserCommand,
  BrowserHeader,
  BrowserNetworkDetail,
  BrowserNetworkEntry,
  BrowserNetworkLog,
  BrowserNetworkSaved,
} from "@zvs/shared";
import {
  mdiCancel,
  mdiContentSaveOutline,
  mdiClose,
  mdiLanConnect,
  mdiMagnify,
  mdiRecord,
  mdiRecordCircleOutline,
} from "@mdi/js";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import Icon from "./Icon";
import IconButton from "./IconButton";
import {
  BODY_BINARY,
  BODY_LOADING,
  BODY_TRUNCATED,
  BODY_UNAVAILABLE,
  CLEAR_LOG,
  COLUMNS,
  DETAIL_CLOSE,
  DETAIL_TABS,
  DEVTOOLS_CLOSE,
  type DetailTab,
  FILTER_LABEL,
  FILTER_PLACEHOLDER,
  FROM_CACHE,
  GENERAL_LABELS,
  NETWORK_EMPTY,
  NETWORK_EMPTY_HINT,
  NETWORK_FAILED,
  NETWORK_IDLE,
  NETWORK_IDLE_HINT,
  NETWORK_NO_TAB,
  NETWORK_NOTHING_FOUND,
  NETWORK_TAB,
  NO_HEADERS,
  RECORD_START,
  RECORD_STOP,
  RECORDING,
  SAVE_FAILED,
  SAVE_LOG,
  REDACTED,
  REDACTED_HINT,
  SECTIONS,
  STATE_FAILED,
  STATE_PENDING,
  TYPE_FILTERS,
  type TypeFilter,
  droppedLabel,
  entryName,
  formatClock,
  formatDuration,
  formatSize,
  matches,
  pretty,
  requestsWord,
  savedLabel,
  transferred,
} from "./networkCopy";

const POLL_MS = 1000;

const EMPTY_LOG: BrowserNetworkLog = {
  tabId: null,
  recording: false,
  revision: 0,
  dropped: 0,
  entries: [],
};

async function call<T>(command: BrowserCommand): Promise<T> {
  return (await window.zvs.call("browser.command", command)) as T;
}

function statusTone(entry: BrowserNetworkEntry): string {
  if (entry.state === "failed" || (entry.status ?? 0) >= 400) return "err";
  if (entry.state === "pending") return "busy";
  return "";
}

function StatusCell({ entry }: { entry: BrowserNetworkEntry }) {
  if (entry.state === "failed") return <span title={entry.error}>{STATE_FAILED}</span>;
  if (entry.status === undefined) return <span className="muted">{STATE_PENDING}</span>;
  return <>{entry.status}</>;
}

function Headers({ title, headers }: { title: string; headers: BrowserHeader[] }) {
  return (
    <section className="detail-section">
      <h3>{title}</h3>
      {headers.length === 0 ? (
        <p className="muted">{NO_HEADERS}</p>
      ) : (
        <dl>
          {headers.map((header, index) => (
            <div key={`${header.name}-${String(index)}`}>
              <dt>{header.name}</dt>
              <dd>
                {header.redacted ? (
                  <span className="redacted" title={REDACTED_HINT}>
                    {REDACTED}
                  </span>
                ) : (
                  header.value
                )}
              </dd>
            </div>
          ))}
        </dl>
      )}
    </section>
  );
}

function Detail({
  detail,
  tab,
  onTab,
  onClose,
}: {
  detail: BrowserNetworkDetail | null;
  tab: DetailTab;
  onTab: (tab: DetailTab) => void;
  onClose: () => void;
}) {
  const tabs = (Object.keys(DETAIL_TABS) as DetailTab[]).filter(
    (id) => id !== "payload" || detail?.postData !== undefined,
  );
  const shown = tabs.includes(tab) ? tab : "headers";
  const entry = detail?.entry;
  return (
    <aside className="network-detail">
      <div className="detail-tabs" role="tablist">
        <IconButton
          path={mdiClose}
          label={DETAIL_CLOSE}
          tooltip="bottom-center"
          iconSize={14}
          onClick={onClose}
        />
        {tabs.map((id) => (
          <button
            key={id}
            type="button"
            role="tab"
            aria-selected={shown === id}
            className="detail-tab"
            onClick={() => onTab(id)}
          >
            {DETAIL_TABS[id]}
          </button>
        ))}
      </div>
      <ScrollArea orientation="both" className="detail-body">
        {!detail || !entry ? (
          <p className="muted">{BODY_LOADING}</p>
        ) : shown === "headers" ? (
          <>
            <section className="detail-section">
              <h3>{SECTIONS.general}</h3>
              <dl>
                <div>
                  <dt>{GENERAL_LABELS.url}</dt>
                  <dd>{entry.url}</dd>
                </div>
                <div>
                  <dt>{GENERAL_LABELS.method}</dt>
                  <dd>{entry.method}</dd>
                </div>
                {entry.status !== undefined && (
                  <div>
                    <dt>{GENERAL_LABELS.status}</dt>
                    <dd>{[entry.status, detail.statusText].filter(Boolean).join(" ")}</dd>
                  </div>
                )}
                <div>
                  <dt>{GENERAL_LABELS.type}</dt>
                  <dd>{entry.resourceType}</dd>
                </div>
                <div>
                  <dt>{GENERAL_LABELS.started}</dt>
                  <dd>{formatClock(entry.startedAt)}</dd>
                </div>
                {entry.error && (
                  <div>
                    <dt>{GENERAL_LABELS.error}</dt>
                    <dd>{entry.error}</dd>
                  </div>
                )}
              </dl>
            </section>
            <Headers title={SECTIONS.response} headers={detail.responseHeaders} />
            <Headers title={SECTIONS.request} headers={detail.requestHeaders} />
          </>
        ) : shown === "payload" ? (
          <pre className="detail-code">{pretty(detail.postData ?? "")}</pre>
        ) : !detail.body ? (
          <p className="muted">{BODY_UNAVAILABLE}</p>
        ) : detail.body.base64 ? (
          <p className="muted">
            {BODY_BINARY} · {formatSize(entry.size)}
          </p>
        ) : (
          <>
            {detail.body.truncated && <p className="muted">{BODY_TRUNCATED}</p>}
            <pre className="detail-code">{pretty(detail.body.text)}</pre>
          </>
        )}
      </ScrollArea>
    </aside>
  );
}

export default function NetworkPanel({
  tabId,
  onClose,
}: {
  tabId: string | null;
  onClose: () => void;
}) {
  const [log, setLog] = useState<BrowserNetworkLog>(EMPTY_LOG);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [saving, setSaving] = useState(false);
  const [filter, setFilter] = useState<TypeFilter>("all");
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<string | null>(null);
  const [detail, setDetail] = useState<BrowserNetworkDetail | null>(null);
  const [detailTab, setDetailTab] = useState<DetailTab>("headers");
  const selectedEntry = log.entries.find((entry) => entry.id === selected);

  useEffect(() => {
    let alive = true;
    let seen = "";
    setSelected(null);
    const pull = async () => {
      try {
        const next = await call<BrowserNetworkLog>({ action: "network" });
        const key = `${next.tabId ?? ""}:${String(next.revision)}`;
        if (alive && key !== seen) {
          seen = key;
          setLog(next);
        }
      } catch {
        if (alive) setError(NETWORK_FAILED);
      }
    };
    void pull();
    const timer = window.setInterval(() => void pull(), POLL_MS);
    return () => {
      alive = false;
      window.clearInterval(timer);
    };
  }, [tabId]);

  useEffect(() => {
    if (selected === null) {
      setDetail(null);
      return;
    }
    let alive = true;
    call<BrowserNetworkDetail>({ action: "networkDetail", id: selected })
      .then((next) => {
        if (alive) setDetail(next);
      })
      .catch(() => {
        if (alive) setSelected(null);
      });
    return () => {
      alive = false;
    };
  }, [selected, selectedEntry?.state]);

  async function act(command: BrowserCommand) {
    setError("");
    setNotice("");
    try {
      setLog(await call<BrowserNetworkLog>(command));
    } catch {
      setError(NETWORK_FAILED);
    }
  }

  async function save() {
    setError("");
    setNotice("");
    setSaving(true);
    try {
      const result = await call<BrowserNetworkSaved>({ action: "networkSave" });
      if (result.saved) setNotice(savedLabel(result.fileName));
    } catch {
      setError(SAVE_FAILED);
    } finally {
      setSaving(false);
    }
  }

  const visible = log.entries.filter((entry) => matches(entry, filter, query));
  const summary = [
    requestsWord(log.entries.length),
    formatSize(transferred(log.entries)),
    log.dropped > 0 ? droppedLabel(log.dropped) : null,
  ]
    .filter(Boolean)
    .join(" · ");

  return (
    <section className="devtools" aria-label={NETWORK_TAB}>
      <div className="devtools-bar">
        <span className="devtools-tab" aria-current="page">
          <Icon path={mdiLanConnect} size={14} />
          {NETWORK_TAB}
        </span>
        <span className="devtools-divider" aria-hidden="true" />
        <IconButton
          path={log.recording ? mdiRecord : mdiRecordCircleOutline}
          label={log.recording ? RECORD_STOP : RECORD_START}
          tooltip="bottom-center"
          className={log.recording ? "recording" : ""}
          aria-pressed={log.recording}
          disabled={tabId === null}
          onClick={() => void act({ action: "networkRecord", recording: !log.recording })}
        />
        <IconButton
          path={mdiContentSaveOutline}
          label={SAVE_LOG}
          tooltip="bottom-center"
          disabled={!log.recording || saving}
          onClick={() => void save()}
        />
        <IconButton
          path={mdiCancel}
          label={CLEAR_LOG}
          tooltip="bottom-center"
          disabled={tabId === null || log.entries.length === 0}
          onClick={() => {
            setSelected(null);
            void act({ action: "networkClear" });
          }}
        />
        <label className="devtools-search">
          <Icon path={mdiMagnify} size={14} />
          <input
            aria-label={FILTER_LABEL}
            placeholder={FILTER_PLACEHOLDER}
            value={query}
            spellCheck={false}
            onChange={(event) => setQuery(event.target.value)}
          />
        </label>
        <div className="type-filters" role="group" aria-label={FILTER_LABEL}>
          {TYPE_FILTERS.map((option) => (
            <button
              key={option.id}
              type="button"
              className="type-filter"
              aria-pressed={filter === option.id}
              onClick={() => setFilter(option.id)}
            >
              {option.label}
            </button>
          ))}
        </div>
        <span className="devtools-summary">
          {log.recording && (
            <span className="recording-dot">
              <span aria-hidden="true" />
              {RECORDING}
            </span>
          )}
          {error || notice || summary}
        </span>
        <IconButton
          path={mdiClose}
          label={DEVTOOLS_CLOSE}
          tooltip="bottom-right"
          iconSize={15}
          onClick={onClose}
        />
      </div>
      <div className={`network ${selected ? "has-detail" : ""}`}>
        <ScrollArea orientation="both" className="network-list">
          {visible.length === 0 ? (
            <div className="network-empty">
              <strong>
                {tabId === null
                  ? NETWORK_NO_TAB
                  : log.entries.length > 0
                    ? NETWORK_NOTHING_FOUND
                    : log.recording
                      ? NETWORK_EMPTY
                      : NETWORK_IDLE}
              </strong>
              {tabId !== null && log.entries.length === 0 && (
                <p>{log.recording ? NETWORK_EMPTY_HINT : NETWORK_IDLE_HINT}</p>
              )}
            </div>
          ) : (
            <table className="network-table">
              <thead>
                <tr>
                  <th className="col-name">{COLUMNS.name}</th>
                  <th className="col-status">{COLUMNS.status}</th>
                  <th className="col-extra col-method">{COLUMNS.method}</th>
                  <th className="col-extra col-type">{COLUMNS.type}</th>
                  <th className="col-extra col-size">{COLUMNS.size}</th>
                  <th className="col-extra col-time">{COLUMNS.time}</th>
                </tr>
              </thead>
              <tbody>
                {visible.map((entry) => {
                  const { name, host } = entryName(entry.url);
                  return (
                    <tr
                      key={entry.id}
                      className={`${statusTone(entry)} ${entry.id === selected ? "selected" : ""}`}
                      aria-selected={entry.id === selected}
                      onClick={() => setSelected(entry.id)}
                    >
                      <td className="col-name">
                        <button
                          type="button"
                          className="entry-name"
                          title={entry.url}
                          onClick={(event) => {
                            event.stopPropagation();
                            setSelected(entry.id);
                          }}
                        >
                          <span>{name}</span>
                          <small>{host}</small>
                        </button>
                      </td>
                      <td className="col-status">
                        <StatusCell entry={entry} />
                      </td>
                      <td className="col-extra col-method">{entry.method}</td>
                      <td className="col-extra col-type">{entry.resourceType}</td>
                      <td className="col-extra col-size">
                        {entry.fromCache ? <span className="muted">{FROM_CACHE}</span> : null}{" "}
                        {formatSize(entry.size)}
                      </td>
                      <td className="col-extra col-time">{formatDuration(entry.duration)}</td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          )}
        </ScrollArea>
        {selected && (
          <Detail
            detail={detail?.entry.id === selected ? detail : null}
            tab={detailTab}
            onTab={setDetailTab}
            onClose={() => setSelected(null)}
          />
        )}
      </div>
    </section>
  );
}
