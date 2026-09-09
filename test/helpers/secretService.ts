import type { DatabaseClient } from "../../apps/studio/src/host/data/client.ts";
import { CryptoService } from "../../apps/studio/src/host/services/CryptoService.ts";
import { SecretService } from "../../apps/studio/src/host/services/SecretService.ts";
import { createFakeCrypto } from "./fakeCrypto.ts";

export function createSecretService(data: DatabaseClient, clock?: () => number): SecretService {
  return new SecretService({ data, crypto: new CryptoService(createFakeCrypto()), clock });
}
