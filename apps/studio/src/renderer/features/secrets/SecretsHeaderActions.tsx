import { mdiPlus } from "@mdi/js";
import { observer } from "mobx-react-lite";
import Button from "../../ui/atoms/buttons/Button";
import Icon from "../../ui/atoms/Icon";
import TextInput from "../../ui/atoms/TextInput";
import { useStore } from "../../stores/useStore";

const IMPORT_HINT = "Импорт .env появится в отдельной задаче — канал ещё не существует.";

function SecretsHeaderActions() {
  const { secrets } = useStore();

  return (
    <div className="flex flex-none items-center gap-3">
      <div className="w-60">
        <TextInput
          preset="search"
          value={secrets.query}
          placeholder="Поиск секретов…"
          aria-label="Поиск секретов"
          onChange={(event) => {
            secrets.setQuery(event.target.value);
          }}
        />
      </div>
      <Button type="button" tone="secondary" disabled title={IMPORT_HINT}>
        Импорт .env
      </Button>
      <Button type="button" tone="primary" onClick={secrets.requestCreate}>
        <Icon path={mdiPlus} size={15} />
        Новый секрет
      </Button>
    </div>
  );
}

export default observer(SecretsHeaderActions);
