import { defineConfig } from "drizzle-kit";

export default defineConfig({
  dialect: "sqlite",
  schema: "./src/host/data/schema/index.ts",
  out: "./src/host/data/migrations",
  casing: "snake_case",
  strict: true,
  verbose: true,
});
