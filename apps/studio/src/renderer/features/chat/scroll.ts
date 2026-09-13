export function shouldPinToBottom(
  scrollTop: number,
  clientHeight: number,
  scrollHeight: number,
): boolean {
  return scrollHeight - clientHeight - scrollTop <= 48;
}
