import { z } from "zod";
import { AppError, AppErrorCode, type Json, type RunGraph } from "@zvs/shared";
import type { NodeDef, StepContext } from "./types.ts";

export interface RegisteredNode {
  readonly sideEffectFree: boolean;
  readonly permission: NodeDef<unknown, unknown>["permission"];
  execute(context: StepContext, input: unknown): Promise<Json>;
}
export class NodeRegistry {
  private readonly nodes = new Map<string, RegisteredNode>();
  register<I, O>(definition: NodeDef<I, O>): void {
    if (this.nodes.has(definition.type)) throw new Error(`Duplicate node type: ${definition.type}`);
    if (typeof definition.sideEffectFree !== "boolean")
      throw new Error("Node must declare sideEffectFree");
    this.nodes.set(definition.type, {
      sideEffectFree: definition.sideEffectFree,
      permission: definition.permission,
      execute: async (context, input) => {
        const output = await definition.run(
          context,
          definition.input.parse(structuredClone(input)),
        );
        return structuredClone(z.json().parse(definition.output.parse(output)));
      },
    });
  }
  get(type: string): RegisteredNode {
    const node = this.nodes.get(type);
    if (!node) throw new AppError(AppErrorCode.VALIDATION_FAILED, `Неизвестный тип узла: ${type}`);
    return node;
  }
  canResume(graph: RunGraph): boolean {
    return graph.nodes.every((node) => this.nodes.get(node.type)?.sideEffectFree === true);
  }
  validate(graph: RunGraph): void {
    const ids = new Set(graph.nodes.map((node) => node.id));
    const invalid = (message: string): never => {
      throw new AppError(AppErrorCode.VALIDATION_FAILED, message);
    };
    if (ids.size !== graph.nodes.length) invalid("Идентификаторы узлов должны быть уникальны");
    for (const node of graph.nodes) {
      this.get(node.type);
      if (new Set(node.dependencies).size !== node.dependencies.length)
        invalid("Повтор зависимости");
      for (const dependency of node.dependencies)
        if (!ids.has(dependency)) invalid("Неизвестная зависимость");
      for (const binding of Object.values(node.bindings)) {
        if (binding.source === "node" && !node.dependencies.includes(binding.nodeId))
          invalid("Источник данных должен быть зависимостью узла");
      }
      if (
        Object.keys(node.bindings).length &&
        (node.input === null || Array.isArray(node.input) || typeof node.input !== "object")
      )
        invalid("Узел с привязками должен принимать объект");
    }
    const completed = new Set<string>();
    while (completed.size < ids.size) {
      const ready = graph.nodes.filter(
        (node) => !completed.has(node.id) && node.dependencies.every((id) => completed.has(id)),
      );
      if (!ready.length) invalid("Граф содержит цикл");
      for (const node of ready) completed.add(node.id);
    }
  }
}
