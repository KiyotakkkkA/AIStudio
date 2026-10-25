import { mdiPowerPlugOffOutline } from "@mdi/js";
import { observer } from "mobx-react-lite";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import Icon from "../../ui/atoms/Icon";
import TextArea from "../../ui/atoms/TextArea";
import TextInput from "../../ui/atoms/TextInput";
import Field from "../../ui/molecules/Field";
import type { SkillFormVm } from "./SkillFormVm";
import type SkillStore from "./SkillStore";
import InstructionEditor from "./InstructionEditor";
import { issueLine, versionLine } from "./skillPresentation";
import Alert from "../../ui/molecules/Alert";

function keyError(error: string | undefined): string | undefined {
  return error === undefined
    ? undefined
    : `Идентификатор: ${error} Измените его в разделе «Дополнительно».`;
}

function SkillManifestView({
  store,
  vm,
}: {
  readonly store: SkillStore;
  readonly vm: SkillFormVm;
}) {
  const skill = vm.isNew ? null : store.selectedSkill;
  const storedIssues = skill?.status === "invalid" && !vm.dirty ? skill.issues : [];

  return (
    <ScrollArea className="flex min-h-0 flex-1 flex-col gap-3.5 px-4.5 py-4">
      {storedIssues.length > 0 ? (
        <Alert title="Навык не может быть использован, пока ошибки не исправлены.">
          {storedIssues.map((issue) => (
            <p key={`${issue.field}:${issue.rule}`} className="m-0">
              {issueLine(issue)}
            </p>
          ))}
        </Alert>
      ) : null}

      {skill !== null && !skill.enabled ? (
        <div className="flex flex-none items-center gap-2.5 rounded-control bg-main-800 px-3 py-2.25 text-[12px] text-main-300">
          <Icon path={mdiPowerPlugOffOutline} size={16} />
          Навык выключен: он не предлагается в чате и не запускается.
        </div>
      ) : null}

      {vm.banner === null ? null : <Alert>{vm.banner}</Alert>}

      <div className="flex-none">
        <Field
          label="Название"
          htmlFor="skill-name"
          required
          help={[
            `Идентификатор: ${vm.key.length > 0 ? vm.key : "—"}`,
            skill === null ? null : versionLine(skill),
          ]
            .filter((part) => part !== null)
            .join(" · ")}
          error={vm.errorOf("name") ?? keyError(vm.errorOf("key"))}
        >
          <TextInput
            id="skill-name"
            value={vm.name}
            placeholder="Отчёт о состоянии провайдеров"
            invalid={vm.errorOf("name") !== undefined}
            onChange={(event) => {
              vm.setName(event.target.value);
            }}
          />
        </Field>
      </div>

      <div className="flex-none">
        <Field
          label="Описание"
          labelAction={
            <span className="ml-1.5 font-normal text-main-500">
              — определяет, когда срабатывает навык
            </span>
          }
          htmlFor="skill-description"
          required
          error={vm.errorOf("description")}
        >
          <TextArea
            id="skill-description"
            value={vm.description}
            placeholder="Что делает навык и в каких запросах он нужен"
            invalid={vm.errorOf("description") !== undefined}
            onChange={(event) => {
              vm.setDescription(event.target.value);
            }}
          />
        </Field>
      </div>

      <div className="flex min-h-60 flex-1 flex-col">
        <span className="mb-1.5 flex items-center text-[12px] font-medium text-main-300">
          Инструкции
          <span className="ml-1 text-err">*</span>
        </span>
        <InstructionEditor
          value={vm.instructions}
          invalid={vm.errorOf("instructions") !== undefined}
          onChange={vm.setInstructions}
        />
        {vm.errorOf("instructions") === undefined ? null : (
          <p className="mt-1.5 text-[11.5px] leading-snug text-err">{vm.errorOf("instructions")}</p>
        )}
      </div>
    </ScrollArea>
  );
}

export default observer(SkillManifestView);
