import { mdiPlus } from "@mdi/js";
import { observer } from "mobx-react-lite";
import Button from "../../ui/atoms/buttons/Button";
import Icon from "../../ui/atoms/Icon";
import { useStore } from "../../stores/useStore";

const IMPORT_HINT = "Импорт .env появится в отдельной задаче — канал ещё не существует.";

function SecretsHeaderActions() {
  const { secrets } = useStore();

  return (
    <div className="flex flex-none items-center gap-3">
      <Button tone="secondary" disabled title={IMPORT_HINT}>
        Импорт .env
      </Button>
      <Button tone="primary" onClick={secrets.requestCreate}>
        <Icon path={mdiPlus} size={15} />
        Новый секрет
      </Button>
    </div>
  );
}

export default observer(SecretsHeaderActions);
