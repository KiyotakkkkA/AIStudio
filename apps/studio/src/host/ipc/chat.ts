import type { Contract } from "@zvs/shared";
import type { IpcHandlers } from "@zvs/ipc";
import type { ChatService } from "../services/ChatService.ts";

export function createChatHandlers(
  service: ChatService,
): Pick<
  IpcHandlers<Contract>,
  | "chat.conversations.list"
  | "chat.conversations.get"
  | "chat.conversations.create"
  | "chat.conversations.remove"
  | "chat.conversations.rename"
  | "chat.send"
  | "chat.cancel"
> {
  return {
    "chat.conversations.list": () => service.list(),
    "chat.conversations.get": ({ id }) => service.get(id),
    "chat.conversations.create": (input) => service.create(input),
    "chat.conversations.remove": ({ id }) => service.remove(id),
    "chat.conversations.rename": ({ id, title }) => service.rename(id, title),
    "chat.send": ({ conversationId, text }) => service.sendMessage(conversationId, text),
    "chat.cancel": ({ id }) => service.cancel(id),
  };
}
