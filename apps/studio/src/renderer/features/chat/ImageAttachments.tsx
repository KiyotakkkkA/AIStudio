import type { ImageAttachmentDto } from "@zvs/shared";
import { mdiClose, mdiImageOutline } from "@mdi/js";
import Icon from "../../ui/atoms/Icon";
import { REMOVE_IMAGE } from "./imageCopy";

export default function ImageAttachments({
  images,
  onRemove,
}: {
  readonly images: readonly ImageAttachmentDto[];
  readonly onRemove?: (id: string) => void;
}) {
  if (images.length === 0) return null;
  return (
    <div className="flex flex-wrap gap-1.5 px-1">
      {images.map((image) => (
        <span
          key={image.id}
          className="inline-flex h-7 items-center gap-1.5 rounded-full bg-main-800 px-2.5 text-[12px] text-main-100"
        >
          <Icon path={mdiImageOutline} size={14} />
          <span className="max-w-48 truncate" title={image.name}>
            {image.name}
          </span>
          {onRemove && (
            <button
              type="button"
              aria-label={`${REMOVE_IMAGE}: ${image.name}`}
              onClick={() => onRemove(image.id)}
              className="flex size-4.5 items-center justify-center rounded-full hover:bg-main-600"
            >
              <Icon path={mdiClose} size={13} />
            </button>
          )}
        </span>
      ))}
    </div>
  );
}
