import { useEffect } from "react";
import { observer } from "mobx-react-lite";
import type { WebSourceDto } from "@zvs/shared";
import { useStore } from "../../stores/useStore";
import { siteInitial } from "./webSources";

const SIZES = {
  sm: { box: "size-3.5", letter: "text-[9px]" },
  md: { box: "size-4", letter: "text-[10px]" },
} as const;

export default observer(function SiteIcon({
  source,
  size = "sm",
}: {
  readonly source: WebSourceDto;
  readonly size?: keyof typeof SIZES;
}) {
  const { chat } = useStore();
  const icons = chat.siteIcons;
  useEffect(() => {
    void icons.load(source);
  }, [icons, source]);
  const url = icons.url(source);
  const { box, letter } = SIZES[size];
  if (url !== null)
    return (
      <img
        src={url}
        alt=""
        className={`inline-block ${box} shrink-0 rounded-sm object-contain align-[-2px]`}
      />
    );
  return (
    <span
      aria-hidden="true"
      className={`inline-flex ${box} ${letter} shrink-0 items-center justify-center rounded-sm bg-main-700 align-[-2px] font-semibold text-main-300`}
    >
      {siteInitial(source)}
    </span>
  );
});
