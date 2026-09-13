import { Children, isValidElement, type ReactNode } from "react";
import ReactMarkdown from "react-markdown";
import { CodeView } from "@kiyotakkkka/zvs-uikit-lib/code-view";

export default function ChatMarkdown({ content }: { readonly content: string }) {
  return (
    <div className="min-w-0 space-y-3 text-[13.5px] leading-relaxed wrap-break-word text-main-100 [&_h1]:text-xl [&_h2]:text-lg [&_h3]:font-semibold [&_li]:my-1 [&_ol]:list-decimal [&_ol]:pl-5 [&_ul]:list-disc [&_ul]:pl-5 [&_blockquote]:border-l-2 [&_blockquote]:border-accent-dark [&_blockquote]:pl-3">
      <ReactMarkdown
        skipHtml
        components={{
          pre({ children }) {
            const child = Children.toArray(children)[0];
            if (!isValidElement<{ children?: ReactNode; className?: string }>(child))
              return <pre>{children}</pre>;
            const code = String(child.props.children ?? "").replace(/\n$/, "");
            const language = /language-([\w+-]+)/.exec(child.props.className ?? "")?.[1] ?? "text";
            return (
              <div className="min-w-0 max-w-full overflow-x-auto">
                <CodeView code={code} language={language} downloadable={false} />
              </div>
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
