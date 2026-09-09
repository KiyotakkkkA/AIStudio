import { AppError, AppErrorCode } from "@zvs/shared";

export interface CryptoBackend {
  isEncryptionAvailable(): boolean;
  encryptString(plainText: string): Buffer;
  decryptString(encrypted: Buffer): string;
}

const UNAVAILABLE =
  "Хранилище ключей операционной системы недоступно, поэтому секрет нельзя ни зашифровать, ни расшифровать. " +
  "Открытые значения не сохраняются никогда.";

export class CryptoService {
  readonly #backend: CryptoBackend;

  constructor(backend: CryptoBackend) {
    this.#backend = backend;
  }

  isAvailable(): boolean {
    try {
      return this.#backend.isEncryptionAvailable();
    } catch {
      return false;
    }
  }

  encrypt(plaintext: string): Buffer {
    this.#requireBackend();
    try {
      return this.#backend.encryptString(plaintext);
    } catch (error: unknown) {
      throw new AppError(AppErrorCode.SECRET_DECRYPT_FAILED, "Не удалось зашифровать секрет", {
        cause: error,
      });
    }
  }

  decrypt(buffer: Buffer): string {
    this.#requireBackend();
    try {
      return this.#backend.decryptString(buffer);
    } catch (error: unknown) {
      throw new AppError(AppErrorCode.SECRET_DECRYPT_FAILED, "Не удалось расшифровать секрет", {
        cause: error,
      });
    }
  }

  #requireBackend(): void {
    if (!this.isAvailable()) {
      throw new AppError(AppErrorCode.SECRET_DECRYPT_FAILED, UNAVAILABLE);
    }
  }
}
