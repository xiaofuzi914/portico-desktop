export interface AppShellRouteParams {
  workspaceId?: string | null;
}

export function hasProjectContext(params: AppShellRouteParams): boolean {
  return Boolean(params.workspaceId?.trim());
}
