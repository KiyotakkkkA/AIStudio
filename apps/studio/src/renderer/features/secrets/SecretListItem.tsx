import type { SecretSummaryDto, SecretTypeSchema } from "@zvs/shared";
import Chip from "../../ui/atoms/Chip";
import Icon from "../../ui/atoms/Icon";
import StatusDot from "../../ui/atoms/StatusDot";
import ListRow from "../../ui/molecules/ListRow";
import {
  maskedHint,
  metaLabel,
  SCOPE_LABELS,
  statusTone,
  typeIcon,
  typeInitials,
  usageLabel,
} from "./secretPresentation";

export interface SecretListItemProps {
  readonly secret: SecretSummaryDto;
  readonly schema?: SecretTypeSchema;
  readonly selected: boolean;
  readonly now: number;
  readonly onSelect: () => void;
}

export default function SecretListItem({
  secret,
  schema,
  selected,
  now,
  onSelect,
}: SecretListItemProps) {
  const icon = typeIcon(secret.type);
  return (
    <ListRow
      selected={selected}
      onClick={onSelect}
      leading={
        <span
          className={`flex size-[32px] flex-none items-center justify-center rounded-[8px] bg-main-700 text-[11px] font-semibold ${
            selected ? "text-accent-medium" : "text-main-300"
          }`}
        >
          {icon === undefined ? typeInitials(secret.type) : <Icon path={icon} size={16} />}
        </span>
      }
      trailing={<StatusDot tone={statusTone(secret)} title={schema?.label ?? secret.type} />}
    >
      <span className="flex items-center gap-[7px]">
        <span className="truncate font-semibold text-main-50">{secret.name}</span>
        <Chip tone={selected ? "raised" : "neutral"}>{secret.type}</Chip>
      </span>
      <span className="truncate font-mono text-[11px] text-main-500">
        {maskedHint(secret.hint)}
      </span>
      <span className="flex gap-[12px] text-[11px] text-main-400">
        <span>{SCOPE_LABELS[secret.scope]}</span>
        <span>{usageLabel(secret.usageCount)}</span>
        <span className="truncate">{metaLabel(secret, now)}</span>
      </span>
    </ListRow>
  );
}
