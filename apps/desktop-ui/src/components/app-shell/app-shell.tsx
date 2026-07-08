import type { ReactNode } from "react";
import { useParams } from "@tanstack/react-router";
import { AppSidebar } from "./app-sidebar";
import { AppTopbar } from "./app-topbar";
import { InspectorShell } from "./inspector-shell";
import { hasProjectContext } from "./app-shell-context";

interface AppShellProps {
  children: ReactNode;
}

export function AppShell({ children }: AppShellProps) {
  const params = useParams({ strict: false }) as { workspaceId?: string };
  const showInspector = hasProjectContext(params);

  return (
    <div className="bg-background text-foreground flex h-screen min-h-screen overflow-hidden">
      <AppSidebar />
      <div className="flex min-w-0 flex-1 flex-col overflow-hidden">
        <AppTopbar />
        <div className="flex min-h-0 flex-1 overflow-hidden">
          <main className="min-w-0 flex-1 overflow-hidden">{children}</main>
          {showInspector && <InspectorShell />}
        </div>
      </div>
    </div>
  );
}
