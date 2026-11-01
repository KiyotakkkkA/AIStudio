import { useId, useState } from "react";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import { mdiChevronDown, mdiThoughtBubbleOutline } from "@mdi/js";
import Icon from "../../ui/atoms/Icon";
import Collapse from "../../ui/motion/Collapse";
import { PipelineItem } from "./Pipeline";

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
  const bodyId = useId();
  const [open, setOpen] = useState(false);
  return (
    <PipelineItem icon={mdiThoughtBubbleOutline} tail={open}>
      <button
        type="button"
        aria-expanded={open}
        aria-controls={bodyId}
        onClick={() => setOpen(!open)}
        className="group/thought flex min-h-8 w-full cursor-pointer items-center gap-2.5 py-1 text-left"
      >
        <span
          className={`shrink-0 text-[12.5px] font-medium text-main-300 transition-colors group-hover/thought:text-main-100 ${
            thinking ? "zvs-shimmer-text" : ""
          }`}
        >
          {thinking ? `${REASONING_TITLE}…` : REASONING_TITLE}
        </span>
        <span className="min-w-0 flex-1 truncate font-mono text-[11px] text-main-500 tabular-nums">
          {reasoningWords(reasoning)} сл.
        </span>
        <Icon
          path={mdiChevronDown}
          size={15}
          className={`shrink-0 text-main-500 group-hover/thought:text-main-200 anim-on:transition-transform ${
            open ? "rotate-180" : ""
          }`}
        />
      </button>
      <Collapse open={open} id={bodyId}>
        <ScrollArea className="mt-1 mb-2 max-h-80 text-[12.5px]/relaxed whitespace-pre-wrap text-main-400">
          {reasoning}
        </ScrollArea>
      </Collapse>
    </PipelineItem>
  );
}
