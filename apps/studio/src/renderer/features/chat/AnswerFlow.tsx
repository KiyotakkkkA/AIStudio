import { Fragment, type ReactNode } from "react";
import type { ChatRoundDto, ChatToolCallDto } from "@zvs/shared";
import ReasoningBlock from "./ReasoningBlock";
import { answerBlocks } from "./answerRounds";
import { PIPELINE_LABEL, PipelineList } from "./Pipeline";
import { TOOL_CALLS_LABEL } from "./toolCallCopy";
import ToolWorkGroup, { workGroupLabel } from "./ToolWorkGroup";

export default function AnswerFlow({
  content,
  reasoning,
  rounds,
  toolCalls,
  generating,
  durationMs = 0,
  renderToolItems,
  renderText,
}: {
  readonly content: string;
  readonly reasoning: string;
  readonly rounds: readonly ChatRoundDto[];
  readonly toolCalls: readonly ChatToolCallDto[];
  readonly generating: boolean;
  readonly durationMs?: number;
  readonly renderToolItems: (calls: readonly ChatToolCallDto[]) => ReactNode;
  readonly renderText: (text: string, last: boolean) => ReactNode;
}) {
  const blocks = answerBlocks(content, reasoning, rounds, toolCalls);
  const units: ReactNode[] = [];
  let pipelineItems: ReactNode[] = [];
  let pipelineKey = "";
  let pipelineReasoning = false;
  let pipelineCalls = 0;
  const flushPipeline = () => {
    if (pipelineItems.length === 0) return;
    const list = (
      <PipelineList label={pipelineReasoning ? PIPELINE_LABEL : TOOL_CALLS_LABEL}>
        {pipelineItems}
      </PipelineList>
    );
    units.push(
      !generating && pipelineCalls > 0 ? (
        <ToolWorkGroup key={pipelineKey} label={workGroupLabel(durationMs, pipelineCalls)}>
          {list}
        </ToolWorkGroup>
      ) : (
        <Fragment key={pipelineKey}>{list}</Fragment>
      ),
    );
    pipelineItems = [];
    pipelineReasoning = false;
    pipelineCalls = 0;
  };
  blocks.forEach((block, index) => {
    if (block.kind === "tools") {
      if (pipelineItems.length === 0) pipelineKey = block.key;
      pipelineCalls += block.calls.length;
      pipelineItems.push(<Fragment key={block.key}>{renderToolItems(block.calls)}</Fragment>);
      return;
    }
    const last = generating && index === blocks.length - 1;
    if (block.reasoning.trim().length > 0) {
      if (pipelineItems.length === 0) pipelineKey = block.key;
      pipelineReasoning = true;
      pipelineItems.push(
        <ReasoningBlock
          key={`${block.key}-reasoning`}
          reasoning={block.reasoning}
          thinking={last && block.text === ""}
        />,
      );
    }
    if (block.text.trim().length === 0) return;
    flushPipeline();
    units.push(<Fragment key={block.key}>{renderText(block.text, last)}</Fragment>);
  });
  flushPipeline();
  return <>{units}</>;
}
