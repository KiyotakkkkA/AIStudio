import type { ReactNode } from "react";
import { Accordion } from "@kiyotakkkka/zvs-uikit-lib";

export default function Disclosure({
  summary,
  defaultOpen = false,
  children,
}: {
  readonly summary: ReactNode;
  readonly defaultOpen?: boolean;
  readonly children: ReactNode;
}) {
  return (
    <Accordion defaultOpen={defaultOpen} className="flex-none border-none bg-transparent">
      <Accordion.Summary className="min-h-8 w-fit gap-1.5 px-0 text-left text-[12px] font-medium text-main-400 transition-colors hover:text-main-100">
        {summary}
      </Accordion.Summary>
      <Accordion.Content className="pt-2 pb-1">{children}</Accordion.Content>
    </Accordion>
  );
}
