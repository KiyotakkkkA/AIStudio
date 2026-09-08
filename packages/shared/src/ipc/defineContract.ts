import type { z } from "zod";

export interface ChannelSchemas<I extends z.ZodType = z.ZodType, O extends z.ZodType = z.ZodType> {
  input: I;
  output: O;
}

export type ContractShape = Record<string, ChannelSchemas>;

export function defineContract<const C extends ContractShape>(contract: C): C {
  return contract;
}

export type ChannelName<C extends ContractShape> = Extract<keyof C, string>;
export type InputOf<C extends ContractShape, K extends ChannelName<C>> = z.infer<C[K]["input"]>;
export type OutputOf<C extends ContractShape, K extends ChannelName<C>> = z.infer<C[K]["output"]>;
