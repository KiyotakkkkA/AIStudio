import { RECAP_TITLE } from "./compactionCopy";

export default function RecapBlock({ recap }: { readonly recap: string }) {
  return (
    <section
      aria-label={RECAP_TITLE}
      className="mt-2 ml-10 rounded-card bg-main-850 px-3.5 py-2.5 text-[12.5px] leading-relaxed"
    >
      <p className="m-0 mb-1 text-[10.5px] font-medium tracking-[0.12em] text-main-400 uppercase">
        {RECAP_TITLE}
      </p>
      <p className="m-0 text-main-200 wrap-break-word whitespace-pre-wrap">{recap}</p>
    </section>
  );
}
