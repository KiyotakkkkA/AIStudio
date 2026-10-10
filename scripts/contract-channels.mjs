import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath, URL } from "node:url";
import process from "node:process";

const target = fileURLToPath(new URL("../packages/shared/contract.channels.json", import.meta.url));
const check = process.argv.includes("--check");

let shared;
try {
  shared = await import("@zvs/shared");
} catch (error) {
  process.stderr.write("Cannot load @zvs/shared. Run `pnpm --filter @zvs/shared build` first.\n");
  throw error;
}

const channels = Object.keys(shared.contract).sort();
const expected = `${JSON.stringify(channels, null, 2)}\n`;

if (check) {
  let current = "";
  try {
    current = readFileSync(target, "utf8");
  } catch {
    current = "";
  }
  if (current.replace(/\r\n/g, "\n") !== expected) {
    process.stderr.write(
      "packages/shared/contract.channels.json is out of date with contract.ts. Run `pnpm contract:channels`.\n",
    );
    process.exit(1);
  }
  process.stdout.write(`contract.channels.json is up to date (${channels.length} channels).\n`);
} else {
  writeFileSync(target, expected);
  process.stdout.write(
    `Wrote ${channels.length} channels to packages/shared/contract.channels.json.\n`,
  );
}
