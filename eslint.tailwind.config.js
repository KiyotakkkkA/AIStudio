import path from "node:path";
import tailwind from "eslint-plugin-better-tailwindcss";
import tseslint from "typescript-eslint";

export default [
  {
    files: ["apps/studio/src/renderer/**/*.{ts,tsx}"],
    languageOptions: {
      parser: tseslint.parser,
    },
    plugins: {
      "better-tailwindcss": tailwind,
    },
    settings: {
      "better-tailwindcss": {
        cwd: path.join(import.meta.dirname, "apps/studio"),
        entryPoint: "src/renderer/app/theme.css",
        rootFontSize: 16,
      },
    },
    rules: {
      "better-tailwindcss/enforce-canonical-classes": "warn",
    },
  },
];
