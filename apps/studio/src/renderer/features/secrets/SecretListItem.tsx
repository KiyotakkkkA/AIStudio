import type { SecretSummaryDto } from "@zvs/shared";
import StatusDot from "../../ui/atoms/StatusDot";
import ListRow from "../../ui/molecules/ListRow";
import { maskedHint, metaLabel, SCOPE_LABELS, statusTone, usageLabel } from "./secretPresentation";

export interface SecretListItemProps {
  readonly secret: SecretSummaryDto;
  readonly selected: boolean;
  readonly now: number;
  readonly onSelect: () => void;
}

export default function SecretListItem({ secret, selected, now, onSelect }: SecretListItemProps) {
  return (
    <ListRow selected={selected} onClick={onSelect}>
      <span className="flex min-w-0 items-center gap-2">
        <StatusDot tone={statusTone(secret)} />
        <span className="flex-1 truncate text-[12.5px] font-medium text-main-50">
          {secret.name}
        </span>
      </span>
      <span className="truncate text-[11px] text-main-400" title={metaLabel(secret, now)}>
        <span className="font-mono text-main-500">{maskedHint(secret.hint)}</span> ·{" "}
        {SCOPE_LABELS[secret.scope]} · {usageLabel(secret.usageCount)}
      </span>
    </ListRow>
  );
}
