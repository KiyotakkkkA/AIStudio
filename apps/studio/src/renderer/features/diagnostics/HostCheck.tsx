import { useCallback, useEffect, useRef, useState } from "react";
import { useStore } from "../../stores/useStore";

const DEMO_STEPS = 5;
const DEMO_JOB_STEPS = 8;
const IDLE = "—";

export default function HostCheck() {
  const { ipc, events } = useStore();
  const [ping, setPing] = useState(IDLE);
  const [stream, setStream] = useState(IDLE);
  const [job, setJob] = useState(IDLE);
  const unsubscribe = useRef<(() => void) | undefined>(undefined);
  const jobUnsubscribe = useRef<(() => void) | undefined>(undefined);

  useEffect(
    () => () => {
      unsubscribe.current?.();
      jobUnsubscribe.current?.();
    },
    [],
  );

  const runPing = useCallback(() => {
    setPing("…");
    void ipc
      .call("system.ping", { sentAt: Date.now() })
      .then((result) => setPing(`pong · ${result.hostTime}`))
      .catch((error: unknown) => setPing(`ошибка · ${String(error)}`));
  }, [ipc]);

  const runStream = useCallback(() => {
    unsubscribe.current?.();
    unsubscribe.current = undefined;
    setStream("…");
    void ipc
      .call("system.demoStream", { steps: DEMO_STEPS })
      .then(({ streamId }) => {
        unsubscribe.current = events.subscribe(streamId, (event) => {
          if (event.type === "progress") setStream(`${event.done}/${event.total}`);
          if (event.type === "end") setStream(`готово · ${event.outcome.status}`);
        });
      })
      .catch((error: unknown) => setStream(`ошибка · ${String(error)}`));
  }, [events, ipc]);

  // The sidecar smoke test: one kernel run over zvs-jobd's job.sleep. It needs the job.run
  // permission, so the first attempt waits for approval on the Tasks page.
  const runJob = useCallback(() => {
    jobUnsubscribe.current?.();
    jobUnsubscribe.current = undefined;
    setJob("…");
    void ipc
      .call("system.demoJob", { steps: DEMO_JOB_STEPS, intervalMs: 250 })
      .then(({ streamId }) => {
        jobUnsubscribe.current = events.subscribe(streamId, (event) => {
          if (event.type === "progress") setJob(`${event.done}/${event.total}`);
          if (event.type === "approval") setJob("ждёт подтверждения · Задачи");
          if (event.type === "end") setJob(`готово · ${event.outcome.status}`);
        });
      })
      .catch((error: unknown) => setJob(`ошибка · ${String(error)}`));
  }, [events, ipc]);

  return (
    <section
      data-testid="host-check"
      className="flex flex-none flex-col gap-3 rounded-card border border-main-750 bg-main-900 p-4"
    >
      <div className="flex flex-col gap-0.5">
        <h2 className="text-[13px] font-semibold text-main-100">Проверка хоста</h2>
        <p className="text-[11.5px] text-main-500">
          Круговой вызов IPC, демонстрационный поток событий и задача в sidecar.
        </p>
      </div>
      <div className="flex flex-wrap items-center gap-2.5">
        <button
          type="button"
          data-testid="ping-button"
          onClick={runPing}
          className="rounded-[6px] border border-main-750 px-3 py-1.5 text-[12px] text-main-200 hover:bg-main-800"
        >
          Пинг
        </button>
        <span data-testid="ping-value" className="font-mono text-[11.5px] text-main-400">
          {ping}
        </span>
      </div>
      <div className="flex flex-wrap items-center gap-2.5">
        <button
          type="button"
          data-testid="stream-button"
          onClick={runStream}
          className="rounded-[6px] border border-main-750 px-3 py-1.5 text-[12px] text-main-200 hover:bg-main-800"
        >
          Поток
        </button>
        <span data-testid="stream-value" className="font-mono text-[11.5px] text-main-400">
          {stream}
        </span>
      </div>
      <div className="flex flex-wrap items-center gap-2.5">
        <button
          type="button"
          data-testid="job-button"
          onClick={runJob}
          className="rounded-[6px] border border-main-750 px-3 py-1.5 text-[12px] text-main-200 hover:bg-main-800"
        >
          Задача
        </button>
        <span data-testid="job-value" className="font-mono text-[11.5px] text-main-400">
          {job}
        </span>
      </div>
    </section>
  );
}
