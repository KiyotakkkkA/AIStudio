import {
  AddGraphEdgeInput,
  type AddGraphEdgeRequest,
  AppError,
  AppErrorCode,
  GraphEdgeDto,
  GraphNeighborDto,
  GraphNeighborsInput,
  type GraphNeighborsRequest,
  GraphPathDto,
  GraphReconcileDto,
  GraphRelationTypeDto,
  GraphShortestPathInput,
  type GraphShortestPathRequest,
  GraphSubgraphDto,
  GraphSubgraphInput,
  type GraphSubgraphRequest,
  RemoveGraphEdgeInput,
} from "@zvs/shared";
import type { UnitOfWork } from "../data/UnitOfWork.ts";
import type {
  GraphEdgeEntity,
  GraphRelationTypeEntity,
  VectorStoreEntity,
} from "../data/schema/index.ts";
import type { GraphCorePort, GraphNode } from "../drivers/rust/graphTypes.ts";
import type { Logger } from "../platform/logger.ts";
import { createId } from "../platform/ids.ts";

export interface GraphServiceOptions {
  data: UnitOfWork;
  core: GraphCorePort;
  logger?: Logger;
  clock?: () => number;
}

export class GraphService {
  private readonly hydrated = new Map<string, Promise<void>>();

  constructor(private readonly options: GraphServiceOptions) {}

  listRelations(): GraphRelationTypeDto[] {
    return this.relations.list().map((row) => GraphRelationTypeDto.parse(row));
  }

  async addEdge(input: AddGraphEdgeRequest): Promise<GraphEdgeDto> {
    const draft = AddGraphEdgeInput.parse(input);
    const store = this.requireStore(draft.storeId);
    const relation = this.requireRelation(draft.relationTypeId);
    const from = this.requireDocument(store.id, draft.fromDocumentId);
    const to = this.requireDocument(store.id, draft.toDocumentId);
    if (from.id === to.id)
      throw new AppError(AppErrorCode.VALIDATION_FAILED, "Документ нельзя связать с самим собой");
    if (this.edges.findByKey(draft))
      throw new AppError(AppErrorCode.CONFLICT, "Такая связь уже существует");
    await this.ensureHydrated(store.id);
    const row = this.edges.link({ ...draft, id: createId(), createdAt: this.now() });
    try {
      await this.options.core.graph.addNode(store.id, this.node(from.id));
      await this.options.core.graph.addNode(store.id, this.node(to.id));
      await this.options.core.graph.addEdge(store.id, {
        from: row.fromDocumentId,
        to: row.toDocumentId,
        relation: relation.name,
        weight: row.weight,
        metadata: row.metadata ?? null,
      });
    } catch (cause) {
      this.edges.remove(row.id);
      this.options.logger?.log("error", "graphRelations", "Не удалось записать связь в граф", {
        storeId: store.id,
        edgeId: row.id,
        error: String(cause),
      });
      throw cause;
    }
    return this.toDto(row);
  }

  async removeEdge(input: RemoveGraphEdgeInput): Promise<GraphEdgeDto> {
    const key = RemoveGraphEdgeInput.parse(input);
    this.requireStore(key.storeId);
    const row = this.edges.findByKey(key);
    if (!row) throw new AppError(AppErrorCode.NOT_FOUND, "Связь не найдена");
    const relation = this.requireRelation(row.relationTypeId);
    await this.ensureHydrated(key.storeId);
    try {
      await this.options.core.graph.removeEdge(
        key.storeId,
        row.fromDocumentId,
        row.toDocumentId,
        relation.name,
      );
    } catch (cause) {
      this.markStale(key.storeId, true);
      this.options.logger?.log("error", "graphRelations", "Не удалось удалить связь из графа", {
        storeId: key.storeId,
        edgeId: row.id,
        error: String(cause),
      });
      throw new AppError(AppErrorCode.NATIVE_ERROR, "Не удалось удалить связь из графа", { cause });
    }
    this.edges.remove(row.id);
    return this.toDto(row);
  }

  async neighbors(input: GraphNeighborsRequest): Promise<GraphNeighborDto[]> {
    const query = GraphNeighborsInput.parse(input);
    await this.prepare(query.storeId, query.documentId);
    const neighbors = await this.options.core.graph.neighbors(query.storeId, {
      id: query.documentId,
      relation: this.relationName(query.relation),
      direction: query.direction,
    });
    return neighbors.map((neighbor) => GraphNeighborDto.parse(neighbor));
  }

  async shortestPath(input: GraphShortestPathRequest): Promise<GraphPathDto | null> {
    const query = GraphShortestPathInput.parse(input);
    await this.prepare(query.storeId, query.fromDocumentId, query.toDocumentId);
    const path = await this.options.core.graph.shortestPath(query.storeId, {
      from: query.fromDocumentId,
      to: query.toDocumentId,
      relation: this.relationName(query.relation),
      direction: query.direction,
    });
    return path === null ? null : GraphPathDto.parse(path);
  }

  async kHopSubgraph(input: GraphSubgraphRequest): Promise<GraphSubgraphDto> {
    const query = GraphSubgraphInput.parse(input);
    await this.prepare(query.storeId, query.documentId);
    const subgraph = await this.options.core.graph.kHopSubgraph(query.storeId, {
      id: query.documentId,
      hops: query.hops,
      relation: this.relationName(query.relation),
      direction: query.direction,
      budget: query.budget,
    });
    return GraphSubgraphDto.parse(subgraph);
  }

  async reconcile(storeId: string): Promise<GraphReconcileDto> {
    const store = this.requireStore(storeId);
    await this.ensureHydrated(store.id);
    const recordedEdges = this.edges.countByStore(store.id);
    const engineEdges = (await this.options.core.graph.stats(store.id)).edgeCount;
    const graphStale = recordedEdges !== engineEdges;
    this.markStale(store.id, graphStale);
    if (graphStale) {
      this.options.logger?.log("warn", "graphRelations", "Граф расходится с базой", {
        storeId: store.id,
        recordedEdges,
        engineEdges,
      });
    }
    return GraphReconcileDto.parse({ storeId: store.id, recordedEdges, engineEdges, graphStale });
  }

  forget(storeId: string): void {
    this.hydrated.delete(storeId);
  }

  private async prepare(storeId: string, ...documentIds: string[]): Promise<void> {
    this.requireStore(storeId);
    for (const documentId of documentIds) this.requireDocument(storeId, documentId);
    await this.ensureHydrated(storeId);
  }

  private ensureHydrated(storeId: string): Promise<void> {
    const pending = this.hydrated.get(storeId);
    if (pending) return pending;
    const work = this.hydrate(storeId).catch((cause: unknown) => {
      this.hydrated.delete(storeId);
      throw cause;
    });
    this.hydrated.set(storeId, work);
    return work;
  }

  private async hydrate(storeId: string): Promise<void> {
    const rows = this.edges.listByStore(storeId);
    const names = new Map(
      this.relations
        .listByIds([...new Set(rows.map((row) => row.relationTypeId))])
        .map((relation) => [relation.id, relation.name] as const),
    );
    const nodes = new Map<string, GraphNode>();
    for (const document of this.documents.listByStore(storeId))
      nodes.set(document.id, this.node(document.id));
    for (const row of rows) {
      nodes.set(row.fromDocumentId, this.node(row.fromDocumentId));
      nodes.set(row.toDocumentId, this.node(row.toDocumentId));
    }
    await this.options.core.graph.hydrate(storeId, {
      nodes: [...nodes.values()],
      edges: rows.map((row) => ({
        from: row.fromDocumentId,
        to: row.toDocumentId,
        relation: this.requireName(names, row),
        weight: row.weight,
        metadata: row.metadata ?? null,
      })),
    });
  }

  private requireName(names: Map<string, string>, row: GraphEdgeEntity): string {
    const name = names.get(row.relationTypeId);
    if (name === undefined)
      throw new AppError(AppErrorCode.NOT_FOUND, "Тип связи не найден в справочнике");
    return name;
  }

  private relationName(relation?: string): string | undefined {
    if (relation === undefined) return undefined;
    const row = this.relations.findByName(relation);
    if (!row) throw new AppError(AppErrorCode.NOT_FOUND, "Тип связи не найден в справочнике");
    return row.name;
  }

  private markStale(storeId: string, graphStale: boolean): void {
    const store = this.stores.findById(storeId);
    if (!store || store.graphStale === graphStale) return;
    this.stores.update(storeId, { graphStale, updatedAt: this.now() });
  }

  private node(id: string): GraphNode {
    return { id, kind: "document" };
  }

  private requireStore(id: string): VectorStoreEntity {
    const store = this.stores.findById(id);
    if (!store) throw new AppError(AppErrorCode.NOT_FOUND, "Векторное хранилище не найдено");
    return store;
  }

  private requireRelation(id: string): GraphRelationTypeEntity {
    const relation = this.relations.findById(id);
    if (!relation) throw new AppError(AppErrorCode.NOT_FOUND, "Тип связи не найден");
    return relation;
  }

  private requireDocument(storeId: string, documentId: string) {
    const document = this.documents.findById(documentId);
    if (!document) throw new AppError(AppErrorCode.NOT_FOUND, "Документ не найден");
    if (document.storeId !== storeId)
      throw new AppError(
        AppErrorCode.VALIDATION_FAILED,
        "Связь возможна только между документами одного хранилища",
      );
    return document;
  }

  private toDto(row: GraphEdgeEntity): GraphEdgeDto {
    return GraphEdgeDto.parse({ ...row, metadata: row.metadata ?? null });
  }

  private now(): number {
    return (this.options.clock ?? Date.now)();
  }

  private get stores() {
    return this.options.data.repositories.vectorStores;
  }
  private get documents() {
    return this.options.data.repositories.vectorDocuments;
  }
  private get relations() {
    return this.options.data.repositories.graphRelations;
  }
  private get edges() {
    return this.options.data.repositories.graphEdges;
  }
}
