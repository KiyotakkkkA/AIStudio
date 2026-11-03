import type { Contract, ConversationId, ImageAttachmentDto } from "@zvs/shared";
import type { IpcClient } from "@zvs/ipc";

const cache = new Map<string, Promise<string | null>>();

/**
 * Resolves a stored attachment to a preview data URL through the host. Copies are immutable,
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
      (data) => data.dataUrl,
      () => null,
    );
    cache.set(key, pending);
  }
  return pending;
}

export function clearImageDataCache(): void {
  cache.clear();
}
