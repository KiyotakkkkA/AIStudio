import { makeAutoObservable, runInAction } from "mobx";
import type { IpcClient } from "@zvs/ipc";
import {
  AddVectorEvalCaseInput,
  VectorEvalRunInput,
  type Contract,
  type VectorEvalCaseDto,
  type VectorEvalRunDto,
  type VectorStoreId,
} from "@zvs/shared";

export default class VectorEvalStore {
  storeId: VectorStoreId | null = null;
  cases: VectorEvalCaseDto[] = [];
  lastRun: VectorEvalRunDto | null = null;
  question = "";
  expected: string[] = [];
  k = "10";
  loading = false;
  busy = false;
  running = false;
  error: string | null = null;
  private revision = 0;

  constructor(private readonly ipc: IpcClient<Contract>) {
    makeAutoObservable<VectorEvalStore, "ipc">(this, { ipc: false }, { autoBind: true });
  }

  get canAdd(): boolean {
    return (
      !this.busy &&
      !this.running &&
      this.storeId !== null &&
      this.question.trim() !== "" &&
      this.expected.length > 0
    );
  }
  get canRun(): boolean {
    return !this.busy && !this.running && this.storeId !== null && this.cases.length > 0;
  }

  set(field: "question" | "k", value: string) {
    this[field] = value;
  }
  addExpected(documentId: string) {
    if (documentId === "" || this.expected.includes(documentId)) return;
    this.expected = [...this.expected, documentId];
  }
  removeExpected(documentId: string) {
    this.expected = this.expected.filter((id) => id !== documentId);
  }

  reset() {
    ++this.revision;
    this.storeId = null;
    this.cases = [];
    this.lastRun = null;
    this.question = "";
    this.expected = [];
    this.running = false;
    this.error = null;
  }

  async load(storeId: VectorStoreId) {
    const revision = ++this.revision;
    this.storeId = storeId;
    this.loading = true;
    this.error = null;
    try {
      const set = await this.ipc.call("vectorStores.eval.list", { id: storeId });
      runInAction(() => {
        if (revision !== this.revision) return;
        this.cases = set.cases;
        this.lastRun = set.lastRun;
      });
    } catch (error) {
      runInAction(() => {
        if (revision === this.revision) this.error = errorCopy(error);
      });
    } finally {
      runInAction(() => {
        if (revision === this.revision) this.loading = false;
      });
    }
  }

  async add() {
    const storeId = this.storeId;
    if (storeId === null || !this.canAdd) return false;
    const input = AddVectorEvalCaseInput.safeParse({
      storeId,
      question: this.question,
      expectedDocumentIds: this.expected,
    });
    if (!input.success) {
      this.error = "Напишите вопрос и выберите от 1 до 20 ожидаемых документов.";
      return false;
    }
    return this.mutate(async () => {
      const added = await this.ipc.call("vectorStores.eval.add", input.data);
      runInAction(() => {
        if (this.storeId !== storeId) return;
        this.cases = [...this.cases, added];
        this.question = "";
        this.expected = [];
      });
    });
  }

  async remove(id: string) {
    const storeId = this.storeId;
    if (storeId === null) return false;
    return this.mutate(async () => {
      await this.ipc.call("vectorStores.eval.remove", { storeId, id });
      runInAction(() => {
        if (this.storeId === storeId) this.cases = this.cases.filter((entry) => entry.id !== id);
      });
    });
  }

  async run() {
    const storeId = this.storeId;
    if (storeId === null || !this.canRun) return false;
    const input = VectorEvalRunInput.safeParse({ storeId, k: Number(this.k) });
    if (!input.success || this.k.trim() === "") {
      this.error = "Глубина проверки — целое число от 1 до 100.";
      return false;
    }
    const revision = this.revision;
    this.running = true;
    this.error = null;
    try {
      const run = await this.ipc.call("vectorStores.eval.run", input.data);
      runInAction(() => {
        if (revision === this.revision) this.lastRun = run;
      });
      return true;
    } catch (error) {
      runInAction(() => {
        if (revision === this.revision) this.error = errorCopy(error);
      });
      return false;
    } finally {
      runInAction(() => {
        if (revision === this.revision) this.running = false;
      });
    }
  }

  private async mutate(work: () => Promise<void>) {
    if (this.busy || this.running) return false;
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
    : "Не удалось выполнить операцию. Попробуйте ещё раз.";
}
