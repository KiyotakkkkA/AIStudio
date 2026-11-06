import type { BrowserImportKind } from "@zvs/shared";
import brave from "../../../../assets/browsers/brave.svg";
import chrome from "../../../../assets/browsers/chrome.svg";
import edge from "../../../../assets/browsers/edge.svg";
import firefox from "../../../../assets/browsers/firefox.svg";
import opera from "../../../../assets/browsers/opera.svg";
import vivaldi from "../../../../assets/browsers/vivaldi.svg";
import yandex from "../../../../assets/browsers/yandex.png";

const BROWSER_ICONS: Record<BrowserImportKind, string> = {
  brave,
  chrome,
  edge,
  firefox,
  opera,
  vivaldi,
  yandex,
};

export default function BrowserAppIcon({ kind }: { readonly kind: BrowserImportKind }) {
  return (
    <img
      src={BROWSER_ICONS[kind]}
      alt=""
      aria-hidden="true"
      width={28}
      height={28}
      className="size-7 flex-none object-contain"
      draggable={false}
    />
  );
}
