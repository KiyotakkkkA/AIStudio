import type { AllowRuleScope } from "@zvs/shared";
import Button from "../../ui/atoms/buttons/Button";
import {
  ALWAYS_ANYWHERE,
  ALWAYS_CHANGES_IN_FOLDER,
  ALWAYS_IN_FOLDER,
  ALWAYS_THESE_FILES,
} from "../execPolicy/rulesCopy";
import type { ToolCallApproval } from "./ChatToolCalls";

export interface AlwaysAllow {
  readonly available: (approval: ToolCallApproval) => boolean;
  readonly allow: (id: string, scope: AllowRuleScope) => void;
}

export default function AlwaysAllowButtons({
  approval,
  always,
  files,
}: {
  readonly approval: ToolCallApproval;
  readonly always: AlwaysAllow | undefined;
  readonly files: boolean;
}) {
  if (always === undefined || !always.available(approval)) return null;
  return (
    <>
      <Button
        size="sm"
        disabled={approval.busy}
        onClick={() => always.allow(approval.id, "folder")}
      >
        {files ? ALWAYS_CHANGES_IN_FOLDER : ALWAYS_IN_FOLDER}
      </Button>
      <Button
        size="sm"
        disabled={approval.busy}
        onClick={() => always.allow(approval.id, "anywhere")}
      >
        {files ? ALWAYS_THESE_FILES : ALWAYS_ANYWHERE}
      </Button>
    </>
  );
}
