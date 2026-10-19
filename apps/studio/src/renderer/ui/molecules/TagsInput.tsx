import { InputSmall } from "@kiyotakkkka/zvs-uikit-lib";
import { useState, type KeyboardEvent } from "react";
import Chip from "../atoms/Chip";

export interface TagsInputProps {
  readonly tags: readonly string[];
  readonly onChange: (tags: string[]) => void;
  readonly placeholder?: string;
  readonly id?: string;
  readonly max?: number;
}

export default function TagsInput({
  tags,
  onChange,
  placeholder = "Добавить тег…",
  id,
  max = 16,
}: TagsInputProps) {
  const [draft, setDraft] = useState("");

  const commit = (): void => {
    const value = draft.trim().slice(0, 32);
    setDraft("");
    if (value.length === 0 || tags.includes(value) || tags.length >= max) return;
    onChange([...tags, value]);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>): void => {
    if (event.key === "Enter" || event.key === ",") {
      event.preventDefault();
      commit();
      return;
    }
    if (event.key === "Backspace" && draft.length === 0 && tags.length > 0) {
      onChange(tags.slice(0, -1));
    }
  };

  return (
    <div className="flex min-h-9 flex-wrap items-center gap-1.5 rounded-control border border-line bg-main-900 px-2.5 py-1.5 text-[13px] focus-within:border-accent-dark focus-within:shadow-[inset_0_0_0_1px_var(--color-accent-dark)] hover:border-line-strong">
      {tags.map((tag) => (
        <Chip
          key={tag}
          tone="raised"
          title="Удалить тег"
          onClick={() => onChange(tags.filter((item) => item !== tag))}
        >
          {tag}
        </Chip>
      ))}
      <InputSmall
        id={id}
        value={draft}
        placeholder={placeholder}
        onChange={(event) => setDraft(event.target.value)}
        onKeyDown={onKeyDown}
        onBlur={commit}
        rounded=""
        className="min-w-20 flex-1"
        classNames={{
          input:
            "h-5.5 border-0 bg-transparent px-0 text-[13px] text-main-100 shadow-none placeholder:text-main-500 focus-visible:border-0 focus-visible:ring-0",
        }}
      />
    </div>
  );
}
