import { z } from "zod";
import { ConversationId } from "./primitives/branded.js";

export const NAVIGATION_STEP_DOMAIN = "navigation";

export const NotificationTargetDto = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("conversation"), conversationId: ConversationId }),
  z.object({ kind: z.literal("downloads") }),
  z.object({ kind: z.literal("scenarios") }),
]);
export type NotificationTargetDto = z.infer<typeof NotificationTargetDto>;
