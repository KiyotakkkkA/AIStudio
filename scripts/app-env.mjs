import { readFileSync } from "node:fs";
import { fileURLToPath, URL } from "node:url";

export const APP_ENV_PATH = fileURLToPath(new URL("../app.env", import.meta.url));

const LINE = /^([A-Z][A-Z0-9_]*)=(.*)$/;
const REQUIRED = ["ZVS_APP_VERSION", "ZVS_APP_CHANNEL"];

export function readAppEnv() {
  const values = {};
  for (const [index, raw] of readFileSync(APP_ENV_PATH, "utf8").split(/\r?\n/).entries()) {
    const line = raw.trim();
    if (line === "" || line.startsWith("#")) continue;
    const match = LINE.exec(line);
    if (!match) throw new Error(`app.env:${index + 1}: expected KEY=value`);
    values[match[1]] = match[2].trim().replace(/^"(.*)"$/, "$1");
  }
  const missing = REQUIRED.filter((key) => !values[key]);
  if (missing.length > 0) throw new Error(`app.env is missing ${missing.join(", ")}`);
  return values;
}

export function appEnvDefine() {
  return { __ZVS_APP_ENV__: JSON.stringify(readAppEnv()) };
}
