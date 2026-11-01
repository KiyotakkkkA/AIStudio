import { StrictMode, useState } from "react";
import { RouterProvider } from "react-router-dom";
import type { RootStore } from "../stores/RootStore";
import ErrorBoundary from "./ErrorBoundary";
import { createAppRouter } from "./router";
import StoreProvider from "./StoreProvider";

export interface RootProps {
  readonly store: RootStore;
}

export default function Root({ store }: RootProps) {
  const [router] = useState(createAppRouter);
  return (
    <StrictMode>
      <ErrorBoundary>
        <StoreProvider store={store}>
          <RouterProvider router={router} />
        </StoreProvider>
      </ErrorBoundary>
    </StrictMode>
  );
}
