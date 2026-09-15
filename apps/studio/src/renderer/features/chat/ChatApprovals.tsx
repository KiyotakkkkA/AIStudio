import { observer } from "mobx-react-lite";
import Button from "../../ui/atoms/buttons/Button";
import type ChatStore from "./ChatStore";

export default observer(function ChatApprovals({ store }: { readonly store: ChatStore }) {
  return (
    <div className="space-y-2">
      {store.approvals.map((request) => (
        <div
          key={request.id}
          role="status"
          className="space-y-2 rounded-card border border-main-750 bg-main-900 p-3"
        >
          <p className="text-xs text-main-300">
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
