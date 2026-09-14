import { Children, isValidElement, type ReactNode } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import { CodeView } from "@kiyotakkkka/zvs-uikit-lib/code-view";

export default function ChatMarkdown({ content }: { readonly content: string }) {
  return (
    <div className="min-w-0 space-y-3 text-[13.5px] leading-relaxed wrap-break-word text-main-100 [&_h1]:text-xl [&_h2]:text-lg [&_h3]:font-semibold [&_li]:my-1 [&_ol]:list-decimal [&_ol]:pl-5 [&_ul]:list-disc [&_ul]:pl-5 [&_blockquote]:border-l-2 [&_blockquote]:border-accent-dark [&_blockquote]:pl-3 [&_table]:w-full [&_table]:border-collapse [&_th]:border-b [&_th]:border-main-750 [&_th]:px-2 [&_th]:py-1.5 [&_th]:text-left [&_th]:text-xs [&_th]:font-medium [&_th]:text-main-300 [&_td]:border-b [&_td]:border-main-750 [&_td]:px-2 [&_td]:py-1.5 [&_td]:align-top [&_td]:text-[13px] [&_tr:last-child_td]:border-b-0">
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
