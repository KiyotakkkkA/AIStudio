import type { CryptoBackend } from "../../apps/studio/src/host/services/CryptoService.ts";

const MASK = 0x5a;

export interface FakeCryptoBackend extends CryptoBackend {
  available: boolean;
  encryptCalls: number;
  decryptCalls: number;
  failNext: boolean;
}

export function createFakeCrypto(available = true): FakeCryptoBackend {
  const backend: FakeCryptoBackend = {
    available,
    encryptCalls: 0,
    decryptCalls: 0,
    failNext: false,
    isEncryptionAvailable: () => backend.available,
    encryptString(plainText: string): Buffer {
      backend.encryptCalls += 1;
      if (backend.failNext) {
        backend.failNext = false;
        throw new Error("keychain refused the write");
      }
      return mask(Buffer.from(plainText, "utf8"));
    },
    decryptString(encrypted: Buffer): string {
      backend.decryptCalls += 1;
      if (backend.failNext) {
        backend.failNext = false;
        throw new Error("keychain refused the read");
      }
      return mask(encrypted).toString("utf8");
    },
  };
  return backend;
}

function mask(buffer: Buffer): Buffer {
  return Buffer.from(buffer.map((byte) => byte ^ MASK));
}
