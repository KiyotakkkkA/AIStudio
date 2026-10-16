export function restoreEnvelope(response: unknown): unknown {
  if (
    typeof response === "object" &&
    response !== null &&
    (response as { ok?: unknown }).ok === true &&
    !("data" in response)
  )
    return { ...response, data: undefined };
  return response;
}
