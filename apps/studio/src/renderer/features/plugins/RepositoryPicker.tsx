import { mdiMinus } from "@mdi/js";
import { observer } from "mobx-react-lite";
import Button from "../../ui/atoms/buttons/Button";
import IconButton from "../../ui/atoms/buttons/IconButton";
import SelectInput from "../../ui/atoms/SelectInput";
import Field from "../../ui/molecules/Field";
import type PluginStore from "./PluginStore";
import type { PluginFormVm } from "./PluginFormVm";

const LOAD_REPOSITORIES = "Загрузить репозитории";
const LOADING_REPOSITORIES = "Загрузка репозиториев…";
const REPOSITORIES_LABEL = "Отслеживаемые репозитории";

function selectedRepositories(raw: unknown): string[] {
  return typeof raw === "string"
    ? [
        ...new Set(
          raw
            .split(/[,\n]/)
            .map((entry) => entry.trim())
            .filter(Boolean),
        ),
      ]
    : [];
}

function RepositoryPicker({
  store,
  vm,
}: {
  readonly store: PluginStore;
  readonly vm: PluginFormVm;
}) {
  const selected = selectedRepositories(vm.values.repositories);
  const repositories = vm.accountId === null ? [] : (store.repositories[vm.accountId] ?? []);
  return (
    <Field label={REPOSITORIES_LABEL} error={vm.errorOf("config.repositories")}>
      <div className="flex items-center gap-2">
        <div className="min-w-0 flex-1">
          <SelectInput
            label={REPOSITORIES_LABEL}
            searchable
            value=""
            placeholder="Добавить репозиторий"
            disabled={selected.length >= 20}
            options={repositories
              .filter((repository) => !selected.includes(repository.id))
              .map((repository) => ({ value: repository.id, label: repository.name }))}
            onChange={(repository) =>
              vm.setValue("repositories", [...selected, repository].join(", "))
            }
          />
        </div>
        <div className="shrink-0 whitespace-nowrap">
          <Button
            tone="secondary"
            disabled={store.loadingRepositories || store.saving}
            onClick={() => void store.loadRepositories()}
          >
            {store.loadingRepositories ? LOADING_REPOSITORIES : LOAD_REPOSITORIES}
          </Button>
        </div>
      </div>
      {selected.map((repository) => (
        <div key={repository} className="flex items-center gap-2 text-[12px] text-main-100">
          <span className="min-w-0 flex-1 truncate">{repository}</span>
          <IconButton
            path={mdiMinus}
            label="Удалить репозиторий"
            onClick={() =>
              vm.setValue(
                "repositories",
                selected.filter((entry) => entry !== repository).join(", "),
              )
            }
          />
        </div>
      ))}
    </Field>
  );
}

export default observer(RepositoryPicker);
