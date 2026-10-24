export const FX_LEVELS = ["none", "reduced", "full"] as const;
export type FxLevel = (typeof FX_LEVELS)[number];
