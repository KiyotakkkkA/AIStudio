import { MailApprovalDto, type MailDraftDto } from "@zvs/shared";

export const MAIL_LEAD = "Требуется подтверждение отправки письма.";
export const SEND_MAIL = "Отправить";
export const REJECT_MAIL = "Отклонить";
export const ACCOUNT_TITLE = "Почтовый ящик";
export const FROM_TITLE = "Отправитель";
export const TO_TITLE = "Получатели";
export const CC_TITLE = "Копия";
export const SUBJECT_TITLE = "Тема";
export const REPLY_TITLE = "Исходное письмо";
export const TEXT_TITLE = "Текст письма";
export const NO_SUBJECT = "Без темы";

export interface MailPreview {
  readonly account: string;
  readonly draft: MailDraftDto;
}

export function mailPreview(request: unknown): MailPreview | null {
  const parsed = MailApprovalDto.safeParse(request);
  return parsed.success ? { account: parsed.data.account, draft: parsed.data.draft } : null;
}
