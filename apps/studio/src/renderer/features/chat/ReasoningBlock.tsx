import { Accordion, ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import { mdiThoughtBubbleOutline } from "@mdi/js";
import Icon from "../../ui/atoms/Icon";

const REASONING_TITLE = "Размышления";

function reasoningWords(reasoning: string): number {
  return reasoning.split(/\s+/u).filter(Boolean).length;
}

export default function ReasoningBlock({
  reasoning,
  thinking,
}: {
  readonly reasoning: string;
  readonly thinking: boolean;
}) {
  return (
    <Accordion preset="ghost">
      <Accordion.Summary className="min-h-9.5 w-auto justify-start gap-2.5 px-0 py-1.5 text-[12px] font-medium text-main-300 transition-colors hover:text-main-50">
        <span className="flex min-w-0 items-center gap-2.5">
          <span
            aria-hidden="true"
            className="flex size-6 shrink-0 items-center justify-center rounded-pill bg-accent-soft text-accent-medium"
          >
            <Icon path={mdiThoughtBubbleOutline} size={14} />
          </span>
          <span className={`truncate ${thinking ? "zvs-shimmer-text" : ""}`}>
            {thinking ? `${REASONING_TITLE}…` : REASONING_TITLE}
          </span>
          <span className="shrink-0 font-mono text-[11px] font-normal text-main-500 tabular-nums">
            {reasoningWords(reasoning)} сл.
          </span>
        </span>
      </Accordion.Summary>
      <Accordion.Content className="px-0 pt-1 pb-1">
        <ScrollArea className="max-h-80 border-l-2 border-accent-line pr-1 pl-3 text-[12.5px]/relaxed whitespace-pre-wrap text-main-400">
          {reasoning}
        </ScrollArea>
      </Accordion.Content>
    </Accordion>
  );
}
