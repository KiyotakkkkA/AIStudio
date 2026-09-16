import { makeAutoObservable, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import {
  AddGraphEdgeInput,
  GraphNeighborsInput,
  RemoveGraphEdgeInput,
  type Contract,
  type GraphEdgeDto,
  type GraphRelationTypeDto,
  type VectorDocumentDto,
  type VectorStoreId,
} from "@zvs/shared";
import {
  GRAPH_EXPANSION_DEFAULT_MAX,
  mergeGraphNeighbors,
  type GraphExpansion,
  type GraphNeighborhood,
} from "./graphExpansion";

const MANUAL_RELATION = "user_linked";
const RELATION_LABELS: Record<string, string> = {
  cites: "Цитирует",
  co_occurs: "Встречается вместе",
  derived_from: "Произошло из",
  user_linked: "Связано вручную",
};
export interface RelationGroup {
  readonly document: VectorDocumentDto;
  readonly outgoing: readonly GraphEdgeDto[];
  readonly incoming: readonly GraphEdgeDto[];
}

export default class GraphRelationStore {
  relations: GraphRelationTypeDto[] = [];
  edges: GraphEdgeDto[] = [];
  storeId: VectorStoreId | null = null;
  fromDocumentId = "";
  toDocumentId = "";
  relationTypeId = "";
  expandEnabled = false;
  expandMax = String(GRAPH_EXPANSION_DEFAULT_MAX);
  loading = false;
  loaded = false;
  busy = false;
  error: string | null = null;
  private revision = 0;

  constructor(private readonly ipc: IpcClient<Contract>) {
    makeAutoObservable<GraphRelationStore, "ipc">(this, { ipc: false }, { autoBind: true });
  }

  get relationNames(): Map<string, string> {
    return new Map(this.relations.map((relation) => [relation.id as string, relation.name]));
  }
  get canAdd(): boolean {
    return (
      !this.busy &&
      this.storeId !== null &&
      this.relationTypeId !== "" &&
      this.fromDocumentId !== "" &&
      this.toDocumentId !== "" &&
      this.fromDocumentId !== this.toDocumentId
    );
  }
  get expandCap(): number {
    const cap = Number(this.expandMax);
    return Number.isInteger(cap) && cap > 0 ? cap : 0;
  }

  relationName(id: string): string {
    const relation = this.relations.find((item) => item.id === id || item.name === id);
    const name = relation?.name ?? id;
    return RELATION_LABELS[name] ?? name;
  }
  set(field: "fromDocumentId" | "toDocumentId" | "relationTypeId" | "expandMax", value: string) {
    this[field] = value;
  }
  setExpandEnabled(enabled: boolean) {
    this.expandEnabled = enabled;
  }

  /** Documents that carry at least one edge, in the order the documents list already uses. */
  groups(documents: readonly VectorDocumentDto[]): RelationGroup[] {
    const groups: RelationGroup[] = [];
    for (const document of documents) {
      const outgoing = this.edges.filter((edge) => edge.fromDocumentId === document.id);
      const incoming = this.edges.filter((edge) => edge.toDocumentId === document.id);
      if (outgoing.length > 0 || incoming.length > 0) groups.push({ document, outgoing, incoming });
    }
    return groups;
  }

  reset() {
    ++this.revision;
    this.storeId = null;
    this.edges = [];
    this.loaded = false;
    this.error = null;
    this.fromDocumentId = "";
    this.toDocumentId = "";
  }

  async load(storeId: VectorStoreId) {
    const revision = ++this.revision;
    this.storeId = storeId;
    this.loading = true;
    this.error = null;
    try {
      const [relations, edges] = await Promise.all([
        this.relations.length > 0
          ? Promise.resolve(this.relations)
          : this.ipc.call("graphRelations.list", undefined),
        this.ipc.call("graphRelations.listEdges", { id: storeId }),
      ]);
      runInAction(() => {
        if (revision !== this.revision) return;
        this.relations = [...relations];
        this.edges = edges;
        if (this.relationTypeId === "")
          this.relationTypeId =
            relations.find((relation) => relation.name === MANUAL_RELATION)?.id ??
            relations[0]?.id ??
            "";
      });
    } catch (error) {
      runInAction(() => {
        if (revision === this.revision) {
          this.edges = [];
          this.error = errorCopy(error);
        }
      });
    } finally {
      runInAction(() => {
        if (revision !== this.revision) return;
        this.loading = false;
        this.loaded = true;
      });
    }
  }

  async addEdge() {
    const input = AddGraphEdgeInput.safeParse({
      storeId: this.storeId,
      fromDocumentId: this.fromDocumentId,
      toDocumentId: this.toDocumentId,
      relationTypeId: this.relationTypeId,
    });
    if (!this.canAdd || !input.success) {
      this.error = "Выберите два разных документа и тип связи.";
      return false;
    }
    return this.mutate(async () => {
      const edge = await this.ipc.call("graphRelations.addEdge", input.data);
      runInAction(() => {
        this.edges = [...this.edges.filter((known) => known.id !== edge.id), edge];
        this.fromDocumentId = "";
        this.toDocumentId = "";
      });
    });
  }

  async removeEdge(edge: GraphEdgeDto) {
    const input = RemoveGraphEdgeInput.safeParse(edge);
    if (!input.success) return false;
    return this.mutate(async () => {
      await this.ipc.call("graphRelations.removeEdge", input.data);
      runInAction(() => {
        this.edges = this.edges.filter((known) => known.id !== edge.id);
      });
    });
  }

  /**
   * One hop out from every ranked document. A graph the host cannot answer for leaves the vector
   * results as they were rather than failing the search.
   */
  async expand(
    storeId: VectorStoreId,
    hits: readonly { documentId: string }[],
    documents: readonly VectorDocumentDto[],
  ): Promise<GraphExpansion[]> {
    const cap = this.expandCap;
    if (!this.expandEnabled || cap === 0 || hits.length === 0) return [];
    const queries = [...new Set(hits.map((hit) => hit.documentId))]
      .map((documentId) =>
        GraphNeighborsInput.safeParse({ storeId, documentId, direction: "both" }),
      )
      .flatMap((parsed) => (parsed.success ? [parsed.data] : []));
    try {
      const neighborhoods: GraphNeighborhood[] = await Promise.all(
        queries.map(async (query) => ({
          documentId: query.documentId,
          neighbors: await this.ipc.call("graphRelations.neighbors", query),
        })),
      );
      return mergeGraphNeighbors(hits, neighborhoods, documents, cap);
    } catch (error) {
      runInAction(() => {
        this.error = errorCopy(error);
      });
      return [];
    }
  }

  private async mutate(work: () => Promise<void>) {
    if (this.busy) return false;
    this.busy = true;
    this.error = null;
    try {
      await work();
      return true;
    } catch (error) {
      runInAction(() => {
        this.error = errorCopy(error);
      });
      return false;
    } finally {
      runInAction(() => {
        this.busy = false;
      });
    }
  }
}

function errorCopy(error: unknown) {
  return error instanceof Error
    ? error.message
    : "Не удалось выполнить операцию со связями. Попробуйте ещё раз.";
}
