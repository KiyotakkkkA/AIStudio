import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, renameSync, rmSync, statSync, writeFileSync } from "node:fs";
import { dirname } from "node:path";
import { AppError, AppErrorCode, type Json } from "@zvs/shared";
import type {
  JobRunOptions,
  SidecarJobsPort,
} from "../../apps/studio/src/host/drivers/sidecar/SidecarDriver.ts";

export interface FakeResource {
  bytes: Uint8Array;
  /** Chunks are handed over one at a time so a test can cancel part-way. */
  chunkSize?: number;
}

interface Attempt {
  url: string;
  targetPath: string;
  resumedFrom: number;
}

/**
 * Stands in for `zvs-jobd`'s `job.download` with the same observable contract: it appends to a
 * `.part` file, resumes from whatever is already there, verifies the checksum before the
 * rename, and leaves the partial behind when cancelled. The Rust implementation is covered by
 * the crate's own tests; this lets the queue and state machine be tested without a sidecar.
 */
export class FakeDownloadJobs implements SidecarJobsPort {
  readonly attempts: Attempt[] = [];
  readonly resources = new Map<string, FakeResource>();
  /** Resolves once a transfer is in flight, so a test can cancel or queue behind it. */
  private waiting: (() => void)[] = [];
  private gates = new Map<string, Promise<void>>();
  private releases = new Map<string, () => void>();

  serve(url: string, bytes: Uint8Array | string, chunkSize?: number): void {
    const body = typeof bytes === "string" ? new TextEncoder().encode(bytes) : bytes;
    this.resources.set(url, { bytes: body, ...(chunkSize === undefined ? {} : { chunkSize }) });
  }

  /** Holds every transfer of this URL open until `release` is called. */
  hold(url: string): void {
    this.gates.set(url, new Promise<void>((resolve) => this.releases.set(url, resolve)));
  }

  release(url: string): void {
    this.releases.get(url)?.();
    this.gates.delete(url);
    this.releases.delete(url);
  }

  /** Waits until at least `count` transfers have started. */
  started(count: number): Promise<void> {
    if (this.attempts.length >= count) return Promise.resolve();
    return new Promise<void>((resolve) => {
      const check = (): void => {
        if (this.attempts.length >= count) resolve();
        else this.waiting.push(check);
      };
      this.waiting.push(check);
    });
  }

  async run(job: string, params: Json, options: JobRunOptions = {}): Promise<Json> {
    if (job !== "job.download") throw new Error(`Unexpected job: ${job}`);
    const { url, targetPath, checksum } = readParams(params);
    const resource = this.resources.get(url);
    if (!resource) throw new AppError(AppErrorCode.NOT_FOUND, `No resource at ${url}`);

    const part = `${targetPath}.part`;
    mkdirSync(dirname(targetPath), { recursive: true });
    const resumedFrom = sizeOf(part);
    this.attempts.push({ url, targetPath, resumedFrom });
    for (const waiter of this.waiting.splice(0)) waiter();

    const total = resource.bytes.byteLength;
    let done = resumedFrom;
    options.onProgress?.({ done, total, rate: 0 });
    const step = resource.chunkSize ?? total;
    while (done < total) {
      if (options.signal?.aborted)
        throw new AppError(AppErrorCode.RUN_CANCELLED, "Операция отменена");
      const next = Math.min(total, done + step);
      writeFileSync(part, resource.bytes.subarray(0, next));
      done = next;
      options.onProgress?.({ done, total, rate: 1_000_000 });
      // The gate opens between chunks, so a held transfer always has a partial file on disk.
      await this.gates.get(url);
    }

    if (checksum) {
      const actual = createHash(checksum.algorithm === "sha256" ? "sha256" : "sha512")
        .update(readFileSync(part))
        .digest("hex");
      if (actual !== checksum.value.toLowerCase()) {
        rmSync(part, { force: true });
        throw new AppError(AppErrorCode.VALIDATION_FAILED, `Checksum mismatch: got ${actual}`);
      }
    }
    renameSync(part, targetPath);
    return { path: targetPath, bytes: total, resumedFrom, resumable: true, checksum: null };
  }
}

export function sha256(bytes: Uint8Array | string): string {
  return createHash("sha256").update(bytes).digest("hex");
}

function sizeOf(path: string): number {
  try {
    return statSync(path).size;
  } catch {
    return 0;
  }
}

function readParams(params: Json): {
  url: string;
  targetPath: string;
  checksum?: { algorithm: "sha256" | "blake3"; value: string };
} {
  if (params === null || typeof params !== "object" || Array.isArray(params))
    throw new Error("Malformed download parameters");
  const record = params as Record<string, Json>;
  const checksum = record.checksum;
  return {
    url: String(record.url),
    targetPath: String(record.targetPath),
    ...(checksum !== null && typeof checksum === "object" && !Array.isArray(checksum)
      ? {
          checksum: {
            algorithm: (checksum as { algorithm: "sha256" | "blake3" }).algorithm,
            value: String((checksum as Record<string, Json>).value),
          },
        }
      : {}),
  };
}
