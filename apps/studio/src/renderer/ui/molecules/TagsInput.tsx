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
    <div className="flex min-h-[34px] flex-wrap items-center gap-[5px] rounded-[6px] border border-main-600 bg-main-900 px-[10px] py-[5px] text-[12.5px] focus-within:border-accent-dark">
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
      <input
        id={id}
        value={draft}
        placeholder={placeholder}
        onChange={(event) => setDraft(event.target.value)}
        onKeyDown={onKeyDown}
        onBlur={commit}
        className="min-w-[80px] flex-1 bg-transparent text-main-100 outline-none placeholder:text-main-500"
      />
    </div>
  );
}
