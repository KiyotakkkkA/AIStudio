import { makeAutoObservable } from "mobx";
import {
  AGENTIC_DEFAULT_MINUTES,
  AGENTIC_DEFAULT_STEPS,
  AGENTIC_DEFAULT_TOKENS,
  AgenticBudgetDto,
  CreateAgenticSessionInput,
  type AgenticPermissionMode,
  type ProviderDto,
} from "@zvs/shared";

export type BudgetField = "steps" | "tokens" | "minutes";

function whole(value: string): number | null {
  const trimmed = value.replace(/\s+/g, "");
  if (trimmed.length === 0) return null;
  return /^\d+$/.test(trimmed) ? Number(trimmed) : Number.NaN;
}

export function draftBudget(
  steps: string,
  tokens: string,
  minutes: string,
): AgenticBudgetDto | null | undefined {
  const values = [whole(steps), whole(tokens), whole(minutes)];
  if (values.every((value) => value === null)) return undefined;
  const parsed = AgenticBudgetDto.safeParse({
    maxSteps: values[0] ?? AGENTIC_DEFAULT_STEPS,
    maxTokens: values[1] ?? AGENTIC_DEFAULT_TOKENS,
    maxMinutes: values[2] ?? AGENTIC_DEFAULT_MINUTES,
  });
  return parsed.success ? parsed.data : null;
}

function defaultModel(provider: ProviderDto): string {
  return (
    provider.models.find((model) => model.available && model.id === provider.defaultModelId)
      ?.externalId ??
    provider.models.find((model) => model.available)?.externalId ??
    ""
  );
}

export default class SessionDraft {
  goal = "";
  workspace: string | null = null;
  providerId = "";
  modelId = "";
  mode: AgenticPermissionMode = "ask";
  budget: Record<BudgetField, string> = { steps: "", tokens: "", minutes: "" };

  constructor() {
    makeAutoObservable(this, {}, { autoBind: true });
  }

  setGoal(goal: string): void {
    this.goal = goal;
  }

  setWorkspace(workspace: string | null): void {
    this.workspace = workspace;
  }

  setMode(mode: AgenticPermissionMode): void {
    this.mode = mode;
  }

  setModel(modelId: string): void {
    this.modelId = modelId;
  }

  setBudget(field: BudgetField, value: string): void {
    this.budget = { ...this.budget, [field]: value };
  }

  selectProvider(provider: ProviderDto): void {
    this.providerId = provider.id;
    this.modelId = defaultModel(provider);
  }

  adopt(providers: readonly ProviderDto[]): void {
    const current = providers.find((provider) => provider.id === this.providerId);
    if (current !== undefined) {
      if (!current.models.some((model) => model.externalId === this.modelId)) {
        this.modelId = defaultModel(current);
      }
      return;
    }
    const first = providers[0];
    if (first === undefined) {
      this.providerId = "";
      this.modelId = "";
    } else {
      this.selectProvider(first);
    }
  }

  get parsedBudget(): AgenticBudgetDto | null | undefined {
    return draftBudget(this.budget.steps, this.budget.tokens, this.budget.minutes);
  }

  get budgetInvalid(): boolean {
    return this.parsedBudget === null;
  }

  get input(): CreateAgenticSessionInput | null {
    const budget = this.parsedBudget;
    if (budget === null || this.workspace === null) return null;
    const parsed = CreateAgenticSessionInput.safeParse({
      goal: this.goal.trim(),
      workspace: this.workspace,
      providerId: this.providerId,
      modelId: this.modelId,
      mode: this.mode,
      ...(budget === undefined ? {} : { budget }),
    });
    return parsed.success ? parsed.data : null;
  }

  clearGoal(): void {
    this.goal = "";
  }
}
