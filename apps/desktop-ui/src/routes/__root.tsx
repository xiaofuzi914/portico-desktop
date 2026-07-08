import { Outlet, createRootRoute } from "@tanstack/react-router";
import { AppShell } from "@/components/app-shell/app-shell";

export const Route = createRootRoute({
  component: RootComponent,
});

function RootComponent() {
  return (
    <AppShell>
      <Outlet />
    </AppShell>
  );
}
