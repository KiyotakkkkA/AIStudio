export type BrowserLifecycleEvent =
  | { type: "link-finished"; url: string }
  | { type: "link-tab-closed"; url: string }
  | { type: "workspace-closed" }
  | { type: "cookies-cleared"; domain: string };

export class BrowserLifecycle {
  readonly #listeners = new Set<(event: BrowserLifecycleEvent) => void>();

  subscribe(listener: (event: BrowserLifecycleEvent) => void): () => void {
    this.#listeners.add(listener);
    return () => {
      this.#listeners.delete(listener);
    };
  }

  emit(event: BrowserLifecycleEvent): void {
    for (const listener of this.#listeners) listener(event);
  }
}
