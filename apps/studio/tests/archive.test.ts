import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { deflateRawSync, gzipSync } from "node:zlib";
import { afterEach, beforeEach, expect, test } from "vitest";
import { AppErrorCode } from "@zvs/shared";
import { temporaryDirectory, type TemporaryDirectory } from "../../../test/helpers/paths.ts";
import { extractArchive, safeMemberPath } from "../src/host/platform/archive.ts";

let workspace: TemporaryDirectory;

beforeEach(() => {
  workspace = temporaryDirectory("archive-");
});
afterEach(() => {
  workspace.dispose();
});

interface Member {
  readonly name: string;
  readonly body: Buffer;
  readonly deflate?: boolean;
}

/** Builds a minimal but real zip: local headers, then a central directory, then the EOCD. */
function buildZip(members: readonly Member[]): Buffer {
  const locals: Buffer[] = [];
  const central: Buffer[] = [];
  let offset = 0;
  for (const member of members) {
    const name = Buffer.from(member.name, "utf8");
    const stored = member.deflate === true ? deflateRawSync(member.body) : member.body;
    const method = member.deflate === true ? 8 : 0;

    const local = Buffer.alloc(30);
    local.writeUInt32LE(0x04034b50, 0);
    local.writeUInt16LE(20, 4);
    local.writeUInt16LE(method, 8);
    local.writeUInt32LE(0, 14);
    local.writeUInt32LE(stored.length, 18);
    local.writeUInt32LE(member.body.length, 22);
    local.writeUInt16LE(name.length, 26);
    local.writeUInt16LE(0, 28);
    locals.push(local, name, stored);

    const entry = Buffer.alloc(46);
    entry.writeUInt32LE(0x02014b50, 0);
    entry.writeUInt16LE(20, 4);
    entry.writeUInt16LE(20, 6);
    entry.writeUInt16LE(method, 10);
    entry.writeUInt32LE(stored.length, 20);
    entry.writeUInt32LE(member.body.length, 24);
    entry.writeUInt16LE(name.length, 28);
    entry.writeUInt32LE(offset, 42);
    central.push(entry, name);
    offset += local.length + name.length + stored.length;
  }
  const directory = Buffer.concat(central);
  const eocd = Buffer.alloc(22);
  eocd.writeUInt32LE(0x06054b50, 0);
  eocd.writeUInt16LE(members.length, 8);
  eocd.writeUInt16LE(members.length, 10);
  eocd.writeUInt32LE(directory.length, 12);
  eocd.writeUInt32LE(offset, 16);
  return Buffer.concat([...locals, directory, eocd]);
}

function tarHeader(name: string, size: number, type = "0", mode = 0o644): Buffer {
  const header = Buffer.alloc(512);
  header.write(name, 0, 100, "utf8");
  header.write(mode.toString(8).padStart(7, "0") + "\0", 100, 8, "utf8");
  header.write("0000000\0", 108, 8, "utf8");
  header.write("0000000\0", 116, 8, "utf8");
  header.write(size.toString(8).padStart(11, "0") + "\0", 124, 12, "utf8");
  header.write("00000000000\0", 136, 12, "utf8");
  header.write("        ", 148, 8, "utf8");
  header.write(type, 156, 1, "utf8");
  header.write("ustar\0", 257, 6, "utf8");
  header.write("00", 263, 2, "utf8");
  return header;
}

function buildTarGz(members: readonly { name: string; body: Buffer; type?: string }[]): Buffer {
  const blocks: Buffer[] = [];
  for (const member of members) {
    blocks.push(tarHeader(member.name, member.body.length, member.type ?? "0"));
    blocks.push(member.body);
    const padding = member.body.length % 512;
    if (padding !== 0) blocks.push(Buffer.alloc(512 - padding));
  }
  blocks.push(Buffer.alloc(1024));
  return gzipSync(Buffer.concat(blocks));
}

test("a zip is unpacked, stored and deflated members alike", async () => {
  const archive = join(workspace.path, "build.zip");
  writeFileSync(
    archive,
    buildZip([
      { name: "llama-server.exe", body: Buffer.from("binary-ish") },
      { name: "lib/ggml.dll", body: Buffer.from("x".repeat(4096)), deflate: true },
    ]),
  );
  const target = join(workspace.path, "out");

  const report = await extractArchive(archive, target);

  expect(report.files).toBe(2);
  expect(readFileSync(join(target, "llama-server.exe"), "utf8")).toBe("binary-ish");
  expect(readFileSync(join(target, "lib", "ggml.dll"), "utf8")).toBe("x".repeat(4096));
});

test("a tar.gz is unpacked, including a file that needs several blocks", async () => {
  const archive = join(workspace.path, "build.tar.gz");
  const large = Buffer.from("y".repeat(3000));
  writeFileSync(
    archive,
    buildTarGz([
      { name: "build/bin/llama-server", body: Buffer.from("elf") },
      { name: "build/bin/libggml.so", body: large },
      { name: "build/bin/link", body: Buffer.alloc(0), type: "2" },
    ]),
  );
  const target = join(workspace.path, "out");

  const report = await extractArchive(archive, target);

  // The symlink member is read past and dropped rather than written.
  expect(report.files).toBe(2);
  expect(readFileSync(join(target, "build", "bin", "llama-server"), "utf8")).toBe("elf");
  expect(readFileSync(join(target, "build", "bin", "libggml.so")).length).toBe(3000);
});

test("a member that points outside the target is refused, not written", () => {
  const target = join(workspace.path, "out");

  expect(safeMemberPath(target, "../escape.exe")).toBeUndefined();
  expect(safeMemberPath(target, "C:\\windows\\system32\\evil.dll")).toBeUndefined();
  expect(safeMemberPath(target, "nested/")).toBeUndefined();
  // A rooted name is re-anchored under the target rather than refused, the way tar does it.
  expect(safeMemberPath(target, "/etc/passwd")).toBe(join(target, "etc", "passwd"));
  expect(safeMemberPath(target, "bin/llama-server")).toBe(join(target, "bin", "llama-server"));
});

test("a traversing zip member leaves nothing behind it", async () => {
  const archive = join(workspace.path, "evil.zip");
  writeFileSync(
    archive,
    buildZip([
      { name: "../escaped.txt", body: Buffer.from("no") },
      { name: "kept.txt", body: Buffer.from("yes") },
    ]),
  );
  const target = join(workspace.path, "out");

  const report = await extractArchive(archive, target);

  expect(report.files).toBe(1);
  expect(readFileSync(join(target, "kept.txt"), "utf8")).toBe("yes");
});

test("an unknown archive extension is refused rather than guessed at", async () => {
  const archive = join(workspace.path, "build.7z");
  writeFileSync(archive, Buffer.from("not an archive we read"));

  await expect(extractArchive(archive, join(workspace.path, "out"))).rejects.toMatchObject({
    code: AppErrorCode.UNSUPPORTED_FORMAT,
  });
});
