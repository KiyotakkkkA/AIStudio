import { Component, type ErrorInfo, type ReactNode } from "react";

export interface ErrorBoundaryProps {
  readonly children: ReactNode;
}

interface ErrorBoundaryState {
  readonly message?: string;
}

export default class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  state: ErrorBoundaryState = {};

  static getDerivedStateFromError(error: unknown): ErrorBoundaryState {
    return { message: error instanceof Error ? error.message : String(error) };
  }

  componentDidCatch(error: unknown, info: ErrorInfo): void {
    console.error("[renderer] Необработанная ошибка интерфейса", error, info.componentStack);
  }

  private readonly reload = () => {
    window.location.reload();
  };

  render(): ReactNode {
    if (this.state.message === undefined) return this.props.children;
    return (
      <div className="flex size-full  items-center justify-center bg-main-900 p-6">
        <div className="flex max-w-130 flex-col gap-3 rounded-card border border-main-700 bg-main-800 p-5">
          <h1 className="text-[15px] font-semibold text-main-50">
            Интерфейс не удалось отрисовать
          </h1>
          <p className="font-mono text-[11.5px] wrap-break-word text-err">{this.state.message}</p>
          <p className="text-[12px] text-main-400">
            Работа приложения продолжается в фоне. Перезагрузите окно, чтобы вернуться к работе.
          </p>
          <button
            type="button"
            onClick={this.reload}
            className="h-8 self-start rounded-[6px] bg-accent-dark px-3.25 text-[12.5px] font-semibold text-main-900"
          >
            Перезагрузить окно
          </button>
        </div>
      </div>
    );
  }
}
