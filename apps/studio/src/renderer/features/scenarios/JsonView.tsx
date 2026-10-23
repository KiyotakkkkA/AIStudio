import { memo } from "react";
import type { Json } from "@zvs/shared";
import { jsonTokens, type JsonTokenKind } from "./jsonTokens";

const TOKEN_CLASSES: Record<JsonTokenKind, string> = {
  key: "text-accent-medium",
  string: "text-main-200",
  number: "text-ok",
  literal: "text-warn",
  punctuation: "text-main-500",
  space: "",
};

function JsonView({ value }: { readonly value: Json }) {
  return (
    <pre className="m-0 font-mono text-[11.5px] leading-[1.75] whitespace-pre-wrap text-main-300 [overflow-wrap:anywhere]">
      {jsonTokens(value).map((token, index) => (
        <span key={index} className={TOKEN_CLASSES[token.kind]}>
          {token.text}
        </span>
      ))}
    </pre>
  );
}

export default memo(JsonView);
