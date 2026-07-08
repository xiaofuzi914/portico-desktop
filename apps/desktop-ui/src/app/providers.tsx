import { useEffect } from "react";
import { RouterProvider } from "@tanstack/react-router";
import { QueryClientProvider } from "@tanstack/react-query";
import { router } from "@/app/router";
import { queryClient } from "@/app/query-client";
import { listenToRuntimeEvents } from "@/lib/tauri-events";
import { I18nProvider } from "@/lib/i18n-react";

export function Providers() {
  useEffect(() => {
    let unlisten: (() => void) | undefined;

    listenToRuntimeEvents()
      .then((fn) => {
        unlisten = fn;
      })
      .catch((error) => {
        console.error("Failed to listen to runtime events", error);
      });

    return () => {
      unlisten?.();
    };
  }, []);

  return (
    <QueryClientProvider client={queryClient}>
      <I18nProvider>
        <RouterProvider router={router} />
      </I18nProvider>
    </QueryClientProvider>
  );
}
