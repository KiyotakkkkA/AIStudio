import { useCallback, useState } from "react";
import type { ImageAttachmentDto } from "@zvs/shared";
import { useStore } from "../../stores/useStore";
import ImageAttachments, { ImagePreview } from "./ImageAttachments";
import { readImageData, type toolImageReferences } from "./imageData";

export default function ToolImages({
  references,
}: {
  readonly references: NonNullable<ReturnType<typeof toolImageReferences>>;
}) {
  const { ipc } = useStore();
  const [selected, setSelected] = useState<ImageAttachmentDto | null>(null);
  const load = useCallback(
    (image: ImageAttachmentDto) => readImageData(ipc, references.owner, image),
    [ipc, references.owner],
  );
  return (
    <div className="space-y-2">
      <ImageAttachments
        images={references.images}
        load={load}
        onOpen={(image) => setSelected(selected?.id === image.id ? null : image)}
      />
      {selected === null ? null : <ImagePreview image={selected} load={load} />}
    </div>
  );
}
