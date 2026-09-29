import { MutationCache, QueryCache, QueryClient } from "@tanstack/react-query";

import { errorMessage } from "./result";
import { toastError } from "./toast";

declare module "@tanstack/react-query" {
  interface Register {
    mutationMeta: {
      /** The component shows the error itself; skip the global toast. */
      inlineError?: boolean;
      /** Toast title on failure (default "Action failed"). */
      errorTitle?: string;
    };
  }
}

/**
 * Rust owns all state; queries are only a cache of it, kept fresh by events,
 * so nothing goes stale on its own and nothing retries behind the user's back.
 */
export function createQueryClient(): QueryClient {
  return new QueryClient({
    queryCache: new QueryCache({
      onError: (error) => toastError("Couldn't load data", errorMessage(error)),
    }),
    mutationCache: new MutationCache({
      onError: (error, _vars, _ctx, mutation) => {
        if (mutation.meta?.inlineError) return;
        toastError(mutation.meta?.errorTitle ?? "Action failed", errorMessage(error));
      },
    }),
    defaultOptions: {
      queries: {
        staleTime: Infinity,
        retry: false,
        refetchOnWindowFocus: false,
      },
      mutations: { retry: false },
    },
  });
}
