import { z } from "zod";
import { ConversationId, RunId } from "./primitives/branded.js";
import { Timestamp } from "./primitives/time.js";

export const MAIL_APPROVAL_KIND = "mail";

const nonEmpty = (max: number) => z.string().min(1).max(max);

export const UntrustedTextDto = z.strictObject({
  text: z.string().max(200000),
  untrusted: z.literal(true),
});
export type UntrustedTextDto = z.infer<typeof UntrustedTextDto>;

const untrusted = (max: number) => UntrustedTextDto.extend({ text: z.string().max(max) });

export const MailRefDto = z.object({
  folder: nonEmpty(1000),
  uidValidity: z.number().int().min(0).max(4294967295),
  uid: z.number().int().min(1).max(4294967295),
});
export type MailRefDto = z.infer<typeof MailRefDto>;

export const MailAttachmentDto = z.object({
  name: untrusted(2000),
  contentType: z.string().max(2000),
  size: z.number().int().nonnegative(),
});
export type MailAttachmentDto = z.infer<typeof MailAttachmentDto>;

export const MailMessageDto = z.object({
  untrusted: z.literal(true),
  ref: MailRefDto,
  messageId: nonEmpty(2000).nullable(),
  from: untrusted(2000),
  fromAddress: nonEmpty(320).nullable(),
  to: z.array(nonEmpty(320)).max(50),
  subject: untrusted(2000),
  date: Timestamp.nullable(),
  inReplyTo: nonEmpty(2000).nullable(),
  references: z.array(nonEmpty(2000)).max(100),
  snippet: untrusted(300),
  attachments: z.array(MailAttachmentDto).max(100),
  size: z.number().int().nonnegative(),
  seen: z.boolean(),
});
export type MailMessageDto = z.infer<typeof MailMessageDto>;

export const MailBodyDto = z.object({
  untrusted: z.literal(true),
  message: MailMessageDto,
  text: UntrustedTextDto,
  truncated: z.boolean(),
});
export type MailBodyDto = z.infer<typeof MailBodyDto>;

export const MailDraftDto = z.object({
  from: nonEmpty(2000),
  to: z.array(nonEmpty(320)).min(1).max(50),
  cc: z.array(nonEmpty(320)).max(50),
  subject: z.string().max(998),
  text: z.string().max(200000),
  inReplyTo: nonEmpty(2000).nullable(),
  references: z.array(nonEmpty(2000)).max(100),
});
export type MailDraftDto = z.infer<typeof MailDraftDto>;

export const MailApprovalDto = z.object({
  id: z.uuid(),
  runId: RunId,
  kind: z.literal(MAIL_APPROVAL_KIND),
  subject: z.string().min(1),
  scope: z.string(),
  expiresAt: z.number().int(),
  conversationId: ConversationId.optional(),
  tool: z.string().min(1),
  action: z.string().max(2000),
  account: nonEmpty(2000),
  draft: MailDraftDto,
});
export type MailApprovalDto = z.infer<typeof MailApprovalDto>;
