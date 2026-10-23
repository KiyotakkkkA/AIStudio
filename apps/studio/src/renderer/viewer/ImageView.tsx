import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { observer } from "mobx-react-lite";
import { ScrollArea } from "@kiyotakkkka/zvs-uikit-lib";
import type ViewerStore from "../stores/ViewerStore";
import { PAGE_PADDING, clampZoom } from "./viewerModel";

function ImageView({
  viewer,
  url,
  name,
}: {
  readonly viewer: ViewerStore;
  readonly url: string;
  readonly name: string;
}) {
  const scrollerRef = useRef<HTMLDivElement>(null);
  const [natural, setNatural] = useState<{ width: number; height: number } | null>(null);
  const [width, setWidth] = useState(0);

  useLayoutEffect(() => {
    const scroller = scrollerRef.current;
    if (scroller === null) return;
    const measure = () => setWidth(scroller.clientWidth);
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(scroller);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    viewer.setPageCount(1);
  }, [viewer]);

  useEffect(() => {
    if (natural !== null && width > 0 && viewer.fitWidth)
      viewer.applyFitZoom(Math.min(1, clampZoom((width - PAGE_PADDING * 2) / natural.width)));
  }, [viewer, viewer.fitWidth, natural, width]);

  const shown = natural === null ? undefined : Math.round(natural.width * viewer.zoom);

  return (
    <ScrollArea ref={scrollerRef} orientation="both" className="min-h-0 flex-1 bg-main-850 p-5">
      <img
        src={url}
        alt={name}
        draggable={false}
        onLoad={(event) =>
          setNatural({
            width: event.currentTarget.naturalWidth,
            height: event.currentTarget.naturalHeight,
          })
        }
        className="mx-auto block max-w-none rounded-[3px] bg-white shadow-pop"
        style={{ width: shown === undefined ? undefined : `${String(shown)}px` }}
      />
    </ScrollArea>
  );
}

export default observer(ImageView);
