import { StrictMode, useState } from "react";
import { RouterProvider } from "react-router-dom";
import type { RootStoreEnvironment } from "../stores/RootStore";
import ErrorBoundary from "./ErrorBoundary";
import { createAppRouter } from "./router";
import StoreProvider from "./StoreProvider";

export interface RootProps {
  readonly environment: RootStoreEnvironment;
}

export default function Root({ environment }: RootProps) {
  const [router] = useState(createAppRouter);
  return (
    <StrictMode>
      <ErrorBoundary>
        <StoreProvider environment={environment}>
          <RouterProvider router={router} />
        </StoreProvider>
      </ErrorBoundary>
    </StrictMode>
  );
}
