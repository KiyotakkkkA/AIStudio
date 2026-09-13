import { asc, desc, eq } from "drizzle-orm";
import {
  conversation,
  message,
  type ConversationInsert,
  type MessageInsert,
} from "../schema/index.ts";
import { Repository } from "./Repository.ts";

export class ChatRepository extends Repository {
  list() {
    return this.db
      .select()
      .from(conversation)
      .orderBy(desc(conversation.updatedAt), desc(conversation.id))
      .all();
  }
  get(id: string) {
    return this.db.select().from(conversation).where(eq(conversation.id, id)).get();
  }
  create(value: ConversationInsert) {
    return this.db.insert(conversation).values(value).returning().get();
  }
  update(id: string, patch: Partial<Pick<ConversationInsert, "title" | "updatedAt">>) {
    return this.db.update(conversation).set(patch).where(eq(conversation.id, id)).returning().get();
  }
  remove(id: string): void {
    this.db.delete(conversation).where(eq(conversation.id, id)).run();
  }
  messages(id: string) {
    return this.db
      .select()
      .from(message)
      .where(eq(message.conversationId, id))
      .orderBy(asc(message.createdAt), asc(message.id))
      .all();
  }
  addMessage(value: MessageInsert) {
    return this.db.insert(message).values(value).returning().get();
  }
  getMessage(id: string) {
    return this.db.select().from(message).where(eq(message.id, id)).get();
  }
  updateMessage(
    id: string,
    patch: Partial<Omit<MessageInsert, "id" | "conversationId" | "createdAt">>,
  ): void {
    this.db.update(message).set(patch).where(eq(message.id, id)).run();
  }
}
