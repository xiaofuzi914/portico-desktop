import { Outlet, createRootRoute } from "@tanstack/react-router";
import { z } from "zod";
import { AppShell } from "@/components/app-shell/app-shell";

const rootSearchSchema = z
  .object({
    workspaceId: z.string().optional(),
    mode: z.string().optional(),
    runId: z.string().optional(),
    inspector: z.string().optional(),
  })
  .passthrough();

export const Route = createRootRoute({
  component: RootComponent,
  validateSearch: rootSearchSchema,
});

function RootComponent() {
  return (
    <AppShell>
      <Outlet />
    </AppShell>
  );
}
