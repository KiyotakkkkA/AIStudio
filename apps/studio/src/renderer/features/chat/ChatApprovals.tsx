import { observer } from "mobx-react-lite";
import Button from "../../ui/atoms/buttons/Button";
import type ChatStore from "./ChatStore";

export default observer(function ChatApprovals({ store }: { readonly store: ChatStore }) {
  return (
    <div className="space-y-2">
      {store.standaloneApprovals.map((request) => (
        <div
          key={request.id}
          role="status"
          className="mb-2 flex flex-wrap items-center gap-3 rounded-card border border-warn-line bg-warn-soft px-3.5 py-3"
        >
          <p className="min-w-0 flex-1 text-[13px] text-main-100">
            Разрешить{" "}
            {request.subject === "llm.generate"
              ? "модели создать ответ"
              : request.subject === "vector.search"
                ? "поиск в подключённом хранилище"
                : request.subject}{" "}
            для этого запроса?
          </p>
          <div className="flex gap-2">
            <Button
              disabled={request.busy}
              tone="primary"
              onClick={() => void store.decideApproval(request.id, true)}
            >
              Разрешить один раз
            </Button>
            <Button
              disabled={request.busy}
              onClick={() => void store.decideApproval(request.id, false)}
            >
              Запретить
            </Button>
          </div>
        </div>
      ))}
    </div>
  );
});
