import { Children, isValidElement, type ReactNode } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import { CodeView } from "@kiyotakkkka/zvs-uikit-lib/code-view";

export default function ChatMarkdown({
  content,
  streaming = false,
}: {
  readonly content: string;
  readonly streaming?: boolean;
}) {
  return (
    <div
      className={`${streaming ? "*:animate-zvs-rise " : ""}min-w-0 space-y-3 text-[13.5px] leading-relaxed wrap-break-word text-main-100 [&_a]:text-accent-medium [&_a]:underline [&_a]:decoration-accent-line [&_a]:underline-offset-2 [&_h1]:text-[20px] [&_h1]:font-semibold [&_h1]:tracking-[-0.02em] [&_h2]:text-[17px] [&_h2]:font-semibold [&_h2]:tracking-[-0.01em] [&_h3]:font-semibold [&_hr]:border-line [&_li]:my-1 [&_li]:marker:text-main-500 [&_ol]:list-decimal [&_ol]:pl-5 [&_ul]:list-disc [&_ul]:pl-5 [&_blockquote]:border-l-2 [&_blockquote]:border-accent-dark [&_blockquote]:pl-3.5 [&_blockquote]:text-main-300 [&_table]:w-full [&_table]:border-collapse [&_th]:border-b [&_th]:border-line [&_th]:px-2 [&_th]:py-1.5 [&_th]:text-left [&_th]:bg-main-850 [&_th]:text-[12px] [&_th]:font-semibold [&_th]:text-main-300 [&_td]:border-b [&_td]:border-line [&_td]:px-2 [&_td]:py-1.5 [&_td]:align-top [&_td]:text-[13px] [&_tr:last-child_td]:border-b-0`}
    >
      <ReactMarkdown
        skipHtml
        remarkPlugins={[remarkGfm]}
        components={{
          pre({ children }) {
            const child = Children.toArray(children)[0];
            if (!isValidElement<{ children?: ReactNode; className?: string }>(child))
              return <pre>{children}</pre>;
            const code = String(child.props.children ?? "").replace(/\n$/, "");
            const language = /language-([\w+-]+)/.exec(child.props.className ?? "")?.[1] ?? "text";
            return (
              <ScrollArea orientation="horizontal" className="min-w-0 max-w-full">
                <CodeView code={code} language={language} downloadable={false} />
              </ScrollArea>
            );
          },
          a({ children }) {
            return <span className="text-accent-medium">{children}</span>;
          },
          img({ alt }) {
            return <span>{alt}</span>;
          },
        }}
      >
        {content}
      </ReactMarkdown>
    </div>
  );
}
