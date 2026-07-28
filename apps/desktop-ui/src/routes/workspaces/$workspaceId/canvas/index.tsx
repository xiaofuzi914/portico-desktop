import { createFileRoute } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { ProjectCanvasPage } from "@/features/canvas/canvas-page";
import { listWorkspaces } from "@/lib/tauri-api";
import { asWorkspaceId } from "@/lib/schemas";
import { workspaceKeys } from "@/lib/query-keys";
import { useMemo } from "react";

export const Route = createFileRoute("/workspaces/$workspaceId/canvas/")({
  component: CanvasRoutePage,
});

function CanvasRoutePage() {
  const { workspaceId: workspaceIdParam } = Route.useParams();
  const workspaceId = asWorkspaceId(workspaceIdParam);

  const { data: workspaces } = useQuery({
    queryKey: workspaceKeys.list(),
    queryFn: listWorkspaces,
  });

  const workspace = useMemo(
    () => workspaces?.find((w) => w.id === workspaceId),
    [workspaces, workspaceId],
  );

  return <ProjectCanvasPage workspaceId={workspaceId} workspaceName={workspace?.name} />;
}
