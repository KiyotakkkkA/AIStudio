import { createReadStream } from "node:fs";
import { chmod, mkdir, open, writeFile } from "node:fs/promises";
import { dirname, join, normalize, sep } from "node:path";
import { Writable } from "node:stream";
import { pipeline } from "node:stream/promises";
import { promisify } from "node:util";
import { createGunzip, inflateRaw } from "node:zlib";
import { AppError, AppErrorCode } from "@zvs/shared";

const inflate = promisify(inflateRaw);

export interface ExtractOptions {
  signal?: AbortSignal;
  /** Called as each member lands, so a long unpack can drive a progress bar. */
  onEntry?: (name: string, done: number, total: number) => void;
}

export interface ExtractReport {
  readonly files: number;
  readonly bytes: number;
}

/**
 * Picks the reader from the file name. llama.cpp ships `.zip` for Windows and `.tar.gz`
 * everywhere else, so those are the two shapes that matter; anything else is refused loudly
 * rather than half-unpacked.
 */
export function extractArchive(
  archivePath: string,
  targetDir: string,
  options: ExtractOptions = {},
): Promise<ExtractReport> {
  const lower = archivePath.toLowerCase();
  if (lower.endsWith(".zip")) return extractZip(archivePath, targetDir, options);
  if (lower.endsWith(".tar.gz") || lower.endsWith(".tgz"))
    return extractTarGz(archivePath, targetDir, options);
  return Promise.reject(
    new AppError(AppErrorCode.UNSUPPORTED_FORMAT, "Неизвестный формат архива", {
      details: { archivePath },
    }),
  );
}

/**
 * A member path is data from a downloaded file, so it is resolved against the target and
 * checked to still be inside it. `..`, absolute paths and drive letters all fail here rather
 * than writing somewhere else on disk.
 */
export function safeMemberPath(targetDir: string, name: string): string | undefined {
  const cleaned = name.replaceAll("\\", "/").replace(/^\/+/, "");
  if (cleaned === "" || cleaned.endsWith("/")) return undefined;
  if (/^[a-zA-Z]:/.test(cleaned)) return undefined;
  const resolved = normalize(join(targetDir, cleaned));
  const root = normalize(targetDir.endsWith(sep) ? targetDir : `${targetDir}${sep}`);
  return resolved.startsWith(root) ? resolved : undefined;
}

async function writeMember(destination: string, content: Buffer, mode: number): Promise<void> {
  await mkdir(dirname(destination), { recursive: true });
  await writeFile(destination, content);
  // A tar from Linux carries the executable bit; a zip only does when it was made on unix.
  if (mode !== 0) await chmod(destination, mode).catch(() => undefined);
}

interface ZipEntry {
  readonly name: string;
  readonly method: number;
  readonly compressedSize: number;
  readonly localOffset: number;
  readonly mode: number;
}

const EOCD_SIGNATURE = 0x06054b50;
const CENTRAL_SIGNATURE = 0x02014b50;
const ZIP64_LOCATOR_SIGNATURE = 0x07064b50;
const MAX_COMMENT = 0xffff;

/** Parses the central directory once, then reads each member's bytes by range. */
async function extractZip(
  archivePath: string,
  targetDir: string,
  options: ExtractOptions,
): Promise<ExtractReport> {
  const handle = await open(archivePath, "r");
  try {
    const { size } = await handle.stat();
    const tailLength = Math.min(size, MAX_COMMENT + 22);
    const tail = Buffer.alloc(tailLength);
    await handle.read(tail, 0, tailLength, size - tailLength);

    let eocd = -1;
    for (let at = tail.length - 22; at >= 0; at -= 1) {
      if (tail.readUInt32LE(at) === EOCD_SIGNATURE) {
        eocd = at;
        break;
      }
    }
    if (eocd < 0)
      throw new AppError(AppErrorCode.UNSUPPORTED_FORMAT, "Архив повреждён: нет оглавления");
    if (eocd >= 20 && tail.readUInt32LE(eocd - 20) === ZIP64_LOCATOR_SIGNATURE)
      throw new AppError(AppErrorCode.UNSUPPORTED_FORMAT, "Архивы ZIP64 не поддерживаются");

    const entryCount = tail.readUInt16LE(eocd + 10);
    const directorySize = tail.readUInt32LE(eocd + 12);
    const directoryOffset = tail.readUInt32LE(eocd + 16);
    const directory = Buffer.alloc(directorySize);
    await handle.read(directory, 0, directorySize, directoryOffset);

    const entries: ZipEntry[] = [];
    let cursor = 0;
    for (let index = 0; index < entryCount && cursor + 46 <= directory.length; index += 1) {
      if (directory.readUInt32LE(cursor) !== CENTRAL_SIGNATURE) break;
      const nameLength = directory.readUInt16LE(cursor + 28);
      const extraLength = directory.readUInt16LE(cursor + 30);
      const commentLength = directory.readUInt16LE(cursor + 32);
      const madeByUnix = directory.readUInt8(cursor + 5) === 3;
      entries.push({
        name: directory.toString("utf8", cursor + 46, cursor + 46 + nameLength),
        method: directory.readUInt16LE(cursor + 10),
        compressedSize: directory.readUInt32LE(cursor + 20),
        localOffset: directory.readUInt32LE(cursor + 42),
        mode: madeByUnix ? (directory.readUInt32LE(cursor + 38) >>> 16) & 0o7777 : 0,
      });
      cursor += 46 + nameLength + extraLength + commentLength;
    }

    let files = 0;
    let bytes = 0;
    for (const entry of entries) {
      options.signal?.throwIfAborted();
      const destination = safeMemberPath(targetDir, entry.name);
      if (destination === undefined) continue;
      if (entry.method !== 0 && entry.method !== 8)
        throw new AppError(AppErrorCode.UNSUPPORTED_FORMAT, "Неподдерживаемый метод сжатия", {
          details: { name: entry.name, method: entry.method },
        });

      // The local header repeats the name and may carry a different extra field, so the data
      // offset is read from it rather than assumed from the central directory.
      const header = Buffer.alloc(30);
      await handle.read(header, 0, 30, entry.localOffset);
      const dataStart = entry.localOffset + 30 + header.readUInt16LE(26) + header.readUInt16LE(28);
      const compressed = Buffer.alloc(entry.compressedSize);
      if (entry.compressedSize > 0)
        await handle.read(compressed, 0, entry.compressedSize, dataStart);
      const content = entry.method === 0 ? compressed : await inflate(compressed);

      await writeMember(destination, content, entry.mode);
      files += 1;
      bytes += content.byteLength;
      options.onEntry?.(entry.name, files, entries.length);
    }
    return { files, bytes };
  } finally {
    await handle.close();
  }
}

const BLOCK = 512;

interface TarMember {
  /** `undefined` for a member whose bytes are read but not written: a link, or a long name. */
  destination: string | undefined;
  longName: boolean;
  remaining: number;
  padding: number;
  mode: number;
  chunks: Buffer[];
}

/**
 * A streaming tar reader fed by gunzip, so a 250 MB archive never sits in memory whole. Only
 * regular files and the GNU long-name header are honoured; links and devices are read past and
 * dropped, which is exactly right for a directory of binaries.
 */
class TarSink {
  private pending: Buffer = Buffer.alloc(0);
  private member: TarMember | null = null;
  private pendingName: string | undefined;
  private readonly writes: Promise<void>[] = [];
  files = 0;
  bytes = 0;

  constructor(
    private readonly targetDir: string,
    private readonly options: ExtractOptions,
  ) {}

  push(chunk: Buffer): void {
    this.pending =
      this.pending.length === 0 ? Buffer.from(chunk) : Buffer.concat([this.pending, chunk]);
    while (this.step());
  }

  async settle(): Promise<void> {
    await Promise.all(this.writes);
  }

  /** One pass; returns false when the buffered bytes cannot advance the machine any further. */
  private step(): boolean {
    const member = this.member;
    if (member !== null) return this.fill(member);
    if (this.pending.length < BLOCK) return false;
    const header = this.pending.subarray(0, BLOCK);
    // Two zero blocks end the archive; one is enough to stop reading members.
    if (header[0] === 0) {
      this.pending = Buffer.alloc(0);
      return false;
    }
    this.pending = this.pending.subarray(BLOCK);
    this.member = this.openMember(header);
    return true;
  }

  private openMember(header: Buffer): TarMember {
    const size = Number.parseInt(readField(header, 124, 12) || "0", 8);
    const mode = Number.parseInt(readField(header, 100, 8) || "0", 8) & 0o7777;
    const type = String.fromCharCode(header[156] ?? 0);
    const padding = size % BLOCK === 0 ? 0 : BLOCK - (size % BLOCK);
    const base = { remaining: size, padding, chunks: [] as Buffer[] };

    if (type === "L") return { ...base, destination: undefined, longName: true, mode: 0 };
    const prefix = readField(header, 345, 155);
    const raw = readField(header, 0, 100);
    const name = this.pendingName ?? (prefix === "" ? raw : `${prefix}/${raw}`);
    this.pendingName = undefined;
    if (type !== "0" && type !== "\0")
      return { ...base, destination: undefined, longName: false, mode: 0 };
    return {
      ...base,
      destination: safeMemberPath(this.targetDir, name),
      longName: false,
      mode,
    };
  }

  private fill(member: TarMember): boolean {
    const take = Math.min(member.remaining, this.pending.length);
    if (take > 0) {
      if (member.destination !== undefined || member.longName)
        member.chunks.push(this.pending.subarray(0, take));
      member.remaining -= take;
      this.pending = this.pending.subarray(take);
    }
    if (member.remaining > 0) return false;
    const skipped = Math.min(member.padding, this.pending.length);
    member.padding -= skipped;
    this.pending = this.pending.subarray(skipped);
    if (member.padding > 0) return false;
    this.member = null;
    this.close(member);
    return true;
  }

  private close(member: TarMember): void {
    if (member.longName) {
      this.pendingName = Buffer.concat(member.chunks).toString("utf8").replace(/\0+$/, "");
      return;
    }
    const destination = member.destination;
    if (destination === undefined) return;
    const content = Buffer.concat(member.chunks);
    this.files += 1;
    this.bytes += content.byteLength;
    this.options.onEntry?.(destination, this.files, 0);
    this.writes.push(writeMember(destination, content, member.mode));
  }
}

async function extractTarGz(
  archivePath: string,
  targetDir: string,
  options: ExtractOptions,
): Promise<ExtractReport> {
  const sink = new TarSink(targetDir, options);
  await pipeline(
    createReadStream(archivePath),
    createGunzip(),
    new Writable({
      write(chunk: Buffer, _encoding, callback) {
        try {
          options.signal?.throwIfAborted();
          sink.push(chunk);
          callback();
        } catch (error: unknown) {
          callback(error as Error);
        }
      },
    }),
    options.signal === undefined ? {} : { signal: options.signal },
  );
  await sink.settle();
  return { files: sink.files, bytes: sink.bytes };
}

function readField(block: Buffer, offset: number, length: number): string {
  const slice = block.subarray(offset, offset + length);
  const end = slice.indexOf(0);
  return slice.toString("utf8", 0, end === -1 ? slice.length : end).trim();
}
