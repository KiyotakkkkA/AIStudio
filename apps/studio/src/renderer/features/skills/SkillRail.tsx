import { observer } from "mobx-react-lite";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import TextInput from "../../ui/atoms/TextInput";
import Field from "../../ui/molecules/Field";
import { KEY_HINT, type SkillFormVm } from "./SkillFormVm";

const HEADING = "text-[10.5px] font-semibold tracking-[0.12em] text-main-500 uppercase";

function SkillRail({ vm }: { readonly vm: SkillFormVm }) {
  return (
    <aside
      aria-label="Параметры навыка"
      className="flex w-75 flex-none flex-col border-l border-line bg-main-800"
    >
      <ScrollArea className="flex min-h-0 flex-1 flex-col gap-3 px-3.5 py-4">
        <span className={HEADING}>Параметры</span>
        <div className="flex flex-col gap-3">
          <Field
            label="Идентификатор"
            htmlFor="skill-key"
            help={KEY_HINT}
            error={vm.errorOf("key")}
          >
            <TextInput
              id="skill-key"
              mono
              value={vm.key}
              invalid={vm.errorOf("key") !== undefined}
              onChange={(event) => {
                vm.setKey(event.target.value);
              }}
            />
          </Field>
        </div>
      </ScrollArea>
    </aside>
  );
}

export default observer(SkillRail);
