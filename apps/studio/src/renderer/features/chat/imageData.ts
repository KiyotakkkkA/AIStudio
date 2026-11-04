import { ConversationId, ImageAttachmentDto, type Contract, type Json } from "@zvs/shared";
import type { IpcClient } from "@zvs/ipc";

const cache = new Map<string, Promise<string | null>>();

export function toolImageReferences(meta: Json | undefined) {
  if (meta === undefined || meta === null || typeof meta !== "object" || Array.isArray(meta))
    return null;
  const owner = ConversationId.safeParse(meta.imageOwner);
  const images = ImageAttachmentDto.array().safeParse(meta.images);
  return owner.success && images.success && images.data.length > 0
    ? { owner: owner.data, images: images.data }
    : null;
}

/** The renderer CSP allows blob: images but not data: URLs, so pixels arrive as a blob. */
function toBlobUrl(dataUrl: string): string | null {
  const comma = dataUrl.indexOf(",");
  const mime = /^data:([^;,]+);base64$/.exec(dataUrl.slice(0, comma))?.[1];
  if (mime === undefined) return null;
  const binary = atob(dataUrl.slice(comma + 1));
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) {
    bytes[index] = binary.charCodeAt(index);
  }
  return URL.createObjectURL(new Blob([bytes], { type: mime }));
}

/**
 * Resolves a stored attachment to a preview blob URL through the host. Copies are immutable,
 * so results are cached per conversation and image; failures resolve to null (placeholder).
 */
export function readImageData(
  ipc: Pick<IpcClient<Contract>, "call">,
  conversationId: ConversationId,
  image: ImageAttachmentDto,
): Promise<string | null> {
  const key = `${conversationId}:${image.id}`;
  let pending = cache.get(key);
  if (pending === undefined) {
    pending = ipc.call("chat.images.read", { id: conversationId, image }).then(
      (data) => toBlobUrl(data.dataUrl),
      () => null,
    );
    cache.set(key, pending);
  }
  return pending;
}

export function clearImageDataCache(): void {
  for (const pending of cache.values()) {
    void pending.then((url) => {
      if (url !== null) URL.revokeObjectURL(url);
    });
  }
  cache.clear();
}
