import { useEffect, useState } from "react";
import type { ImageAttachmentDto } from "@zvs/shared";
import { mdiClose, mdiImageOffOutline } from "@mdi/js";
import Icon from "../../ui/atoms/Icon";
import { IMAGE_UNAVAILABLE, OPEN_IMAGE, REMOVE_IMAGE } from "./imageCopy";

export type ImageDataLoader = (image: ImageAttachmentDto) => Promise<string | null>;

export function useImageData(
  load: ImageDataLoader,
  image: ImageAttachmentDto,
): { readonly dataUrl: string | null; readonly failed: boolean } {
  const [state, setState] = useState<{ dataUrl: string | null; failed: boolean }>({
    dataUrl: null,
    failed: false,
  });
  useEffect(() => {
    let alive = true;
    setState({ dataUrl: null, failed: false });
    void load(image).then((dataUrl) => {
      if (alive) setState({ dataUrl, failed: dataUrl === null });
    });
    return () => {
      alive = false;
    };
  }, [load, image]);
  return state;
}

function Thumb({
  image,
  load,
  size,
  onOpen,
  onRemove,
}: {
  readonly image: ImageAttachmentDto;
  readonly load: ImageDataLoader;
  readonly size: number;
  readonly onOpen?: (image: ImageAttachmentDto) => void;
  readonly onRemove?: (id: string) => void;
}) {
  const { dataUrl, failed } = useImageData(load, image);
  const content = failed ? (
    <span
      role="img"
      aria-label={`${IMAGE_UNAVAILABLE}: ${image.name}`}
      title={`${IMAGE_UNAVAILABLE}: ${image.name}`}
      className="flex size-full items-center justify-center text-main-500"
    >
      <Icon path={mdiImageOffOutline} size={Math.max(16, Math.round(size * 0.4))} />
    </span>
  ) : dataUrl === null ? null : (
    <img src={dataUrl} alt={image.name} draggable={false} className="size-full object-cover" />
  );
  const frame =
    "block size-full overflow-hidden rounded-lg bg-main-800 fx-on:shadow-[inset_0_0_0_1px_var(--color-line)]";
  return (
    <span className="relative inline-block shrink-0" style={{ width: size, height: size }}>
      {onOpen === undefined ? (
        <span title={image.name} className={frame}>
          {content}
        </span>
      ) : (
        <button
          type="button"
          aria-label={`${OPEN_IMAGE}: ${image.name}`}
          title={image.name}
          onClick={() => onOpen(image)}
          className={`${frame} cursor-zoom-in`}
        >
          {content}
        </button>
      )}
      {onRemove === undefined ? null : (
        <button
          type="button"
          aria-label={`${REMOVE_IMAGE}: ${image.name}`}
          onClick={() => onRemove(image.id)}
          className="absolute -top-1.5 -right-1.5 flex size-5 items-center justify-center rounded-full bg-main-600 text-main-100 shadow-pop hover:bg-main-500"
        >
          <Icon path={mdiClose} size={12} />
        </button>
      )}
    </span>
  );
}

export default function ImageAttachments({
  images,
  load,
  size = 80,
  className = "",
  onOpen,
  onRemove,
}: {
  readonly images: readonly ImageAttachmentDto[];
  readonly load: ImageDataLoader;
  readonly size?: number;
  readonly className?: string;
  readonly onOpen?: (image: ImageAttachmentDto) => void;
  readonly onRemove?: (id: string) => void;
}) {
  if (images.length === 0) return null;
  return (
    <div className={`flex flex-wrap gap-2 px-1 ${className}`}>
      {images.map((image) => (
        <Thumb
          key={image.id}
          image={image}
          load={load}
          size={size}
          {...(onOpen === undefined ? {} : { onOpen })}
          {...(onRemove === undefined ? {} : { onRemove })}
        />
      ))}
    </div>
  );
}

export function ImagePreview({
  image,
  load,
}: {
  readonly image: ImageAttachmentDto;
  readonly load: ImageDataLoader;
}) {
  const { dataUrl, failed } = useImageData(load, image);
  if (failed) {
    return (
      <span
        role="img"
        aria-label={`${IMAGE_UNAVAILABLE}: ${image.name}`}
        className="flex h-44 w-72 flex-col items-center justify-center gap-2 rounded-card bg-main-800 text-main-400"
      >
        <Icon path={mdiImageOffOutline} size={32} />
        <span className="text-[12.5px]">{IMAGE_UNAVAILABLE}</span>
      </span>
    );
  }
  if (dataUrl === null) {
    return <span className="block h-44 w-72 animate-pulse rounded-card bg-main-800" />;
  }
  return (
    <img
      src={dataUrl}
      alt={image.name}
      draggable={false}
      className="max-h-[70vh] max-w-full rounded-card object-contain shadow-pop"
    />
  );
}
