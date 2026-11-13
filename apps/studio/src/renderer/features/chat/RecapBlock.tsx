import { RECAP_TITLE } from "./compactionCopy";

export default function RecapBlock({ id, recap }: { readonly id: string; readonly recap: string }) {
  return (
    <section
      id={id}
      aria-label={RECAP_TITLE}
      className="mt-2 text-[12.5px] leading-relaxed text-main-400"
    >
      <p className="m-0 mb-1 text-[10.5px] font-medium tracking-[0.12em] text-main-400 uppercase">
        {RECAP_TITLE}
      </p>
      <p className="m-0 wrap-break-word whitespace-pre-wrap">{recap}</p>
    </section>
  );
}
