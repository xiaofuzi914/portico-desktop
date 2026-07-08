import { createFileRoute, Link, useNavigate } from "@tanstack/react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ArrowRight, FilePlus, FolderOpen, Plus, ShieldCheck } from "lucide-react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { z } from "zod";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { createWorkspace, listWorkspaces } from "@/lib/tauri-api";
import { useState } from "react";
import { useTranslation } from "@/lib/i18n-react";
import { typography } from "@/components/ui/typography";
import { deriveProjectNameFromPath, normalizeDirectorySelection } from "@/lib/path-picker";
import { buildProjectOverviewItems } from "./-overview-model";
import { formatRelativeTime } from "@/lib/formatters";
import { workspaceKeys } from "@/lib/query-keys";

const searchSchema = z.object({
  mode: z.enum(["new"]).optional(),
});

export const Route = createFileRoute("/workspaces/")({
  component: WorkspacesPage,
  validateSearch: searchSchema,
});

function WorkspacesPage() {
  const { mode } = Route.useSearch();
  const { t } = useTranslation();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const [addProjectError, setAddProjectError] = useState<string | null>(null);
  const { data: workspaces, isLoading: workspacesLoading } = useQuery({
    queryKey: workspaceKeys.list(),
    queryFn: listWorkspaces,
  });

  const addExistingProject = useMutation({
    mutationFn: async () => {
      setAddProjectError(null);
      const selection = await openDialog({
        directory: true,
        multiple: false,
        title: t("projects.openExistingProject"),
      });
      const rootPath = normalizeDirectorySelection(selection);
      if (!rootPath) return null;
      return createWorkspace(deriveProjectNameFromPath(rootPath), rootPath, false);
    },
    onSuccess: (workspace) => {
      void queryClient.invalidateQueries({ queryKey: workspaceKeys.list() });
      if (workspace) {
        void navigate({ to: "/workspaces/$workspaceId", params: { workspaceId: workspace.id } });
      }
    },
    onError: (err) => {
      setAddProjectError(err instanceof Error ? err.message : String(err));
    },
  });

  return (
    <main className="flex h-full flex-col overflow-hidden">
      <section className="border-b px-6 py-5">
        <div className="flex items-center gap-3">
          <div className="bg-muted flex h-10 w-10 items-center justify-center rounded-lg border">
            <FolderOpen className="h-5 w-5" />
          </div>
          <div>
            <h1 className={typography.pageTitle}>{t("projects.title")}</h1>
            <p className={typography.pageDescription}>{t("projects.description")}</p>
          </div>
        </div>
      </section>

      <section className="min-h-0 flex-1 overflow-auto px-6 py-10">
        {mode === "new" ? (
          <NewProjectForm />
        ) : workspacesLoading ? (
          <p className="text-muted-foreground mx-auto max-w-5xl text-sm">
            {t("sidebar.loadingProjects")}
          </p>
        ) : workspaces?.length ? (
          <ProjectOverview workspaces={workspaces} />
        ) : (
          <ProjectStartGuide
            onOpenExistingProject={() => addExistingProject.mutate()}
            onCreateNewProject={() => {
              void navigate({ to: "/workspaces", search: { mode: "new" } });
            }}
            isOpeningExistingProject={addExistingProject.isPending}
            error={addProjectError}
          />
        )}
      </section>
    </main>
  );
}

interface ProjectOverviewProps {
  workspaces: Awaited<ReturnType<typeof listWorkspaces>>;
}

export function ProjectOverview({ workspaces }: ProjectOverviewProps) {
  const { t } = useTranslation();
  const items = buildProjectOverviewItems(workspaces);

  return (
    <div className="mx-auto max-w-5xl space-y-5">
      <div className="flex flex-col gap-2 sm:flex-row sm:items-end sm:justify-between">
        <div>
          <h2 className={typography.pageTitle}>{t("projects.overviewTitle")}</h2>
          <p className={typography.pageDescription}>{t("projects.overviewBody")}</p>
        </div>
        <span className={typography.metadata}>
          {items.length} {t("common.total")}
        </span>
      </div>

      <div className="grid gap-3 lg:grid-cols-2">
        {items.map((item) => (
          <Link
            key={item.workspace.id}
            to="/workspaces/$workspaceId"
            params={{ workspaceId: item.workspace.id }}
            className="hover:bg-muted/50 focus-visible:ring-ring rounded-lg border p-4 transition-colors focus-visible:ring-2 focus-visible:outline-none"
          >
            <div className="flex items-start justify-between gap-4">
              <div className="min-w-0">
                <h3 className={`truncate ${typography.sectionTitle}`}>{item.workspace.name}</h3>
                <p className={`mt-1 truncate font-mono ${typography.metadata}`}>
                  {item.workspace.root_path}
                </p>
              </div>
              <ArrowRight className="text-muted-foreground mt-1 h-4 w-4 shrink-0" />
            </div>

            <div className="mt-4 grid gap-2 text-sm sm:grid-cols-2">
              <ProjectFact label={t("projects.trustStatus")}>
                <span className="inline-flex items-center gap-1">
                  <ShieldCheck className="h-3.5 w-3.5" />
                  {item.isTrusted ? t("home.trusted") : t("home.untrusted")}
                </span>
              </ProjectFact>
              <ProjectFact label={t("projects.lastUpdated")}>
                {formatRelativeTime(item.workspace.updated_at)}
              </ProjectFact>
              <ProjectFact label={t("home.readPaths")}>{item.readPathCount}</ProjectFact>
              <ProjectFact label={t("home.writePaths")}>{item.writePathCount}</ProjectFact>
            </div>
          </Link>
        ))}
      </div>
    </div>
  );
}

function ProjectFact({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="min-w-0 rounded-md bg-muted/40 px-3 py-2">
      <p className={typography.metadata}>{label}</p>
      <p className="mt-1 truncate text-sm font-medium">{children}</p>
    </div>
  );
}

export interface ProjectStartGuideProps {
  onOpenExistingProject: () => void;
  onCreateNewProject: () => void;
  isOpeningExistingProject?: boolean;
  error?: string | null;
}

export function ProjectStartGuide({
  onOpenExistingProject,
  onCreateNewProject,
  isOpeningExistingProject = false,
  error,
}: ProjectStartGuideProps) {
  const { t } = useTranslation();

  return (
    <div className="mx-auto max-w-2xl">
      <div className="space-y-5">
        <div className="bg-muted flex h-12 w-12 items-center justify-center rounded-lg border">
          <Plus className="h-5 w-5" />
        </div>
        <div className="space-y-2">
          <h2 className={typography.pageTitle}>{t("projects.startGuideTitle")}</h2>
          <p className={typography.pageDescription}>{t("projects.startGuideBody")}</p>
        </div>
        <div className="grid gap-3 sm:grid-cols-2">
          <button
            type="button"
            className="hover:bg-muted focus-visible:ring-ring rounded-lg border p-4 text-left transition-colors focus-visible:ring-2 focus-visible:outline-none disabled:cursor-wait disabled:opacity-70"
            onClick={onOpenExistingProject}
            disabled={isOpeningExistingProject}
          >
            <FolderOpen className="mb-3 h-4 w-4" />
            <h3 className={typography.sectionTitle}>{t("projects.openExistingProject")}</h3>
            <p className="text-muted-foreground mt-2 text-sm leading-6">
              {t("projects.startGuideOpenExisting")}
            </p>
          </button>
          <button
            type="button"
            className="hover:bg-muted focus-visible:ring-ring rounded-lg border p-4 text-left transition-colors focus-visible:ring-2 focus-visible:outline-none"
            onClick={onCreateNewProject}
          >
            <FilePlus className="mb-3 h-4 w-4" />
            <h3 className={typography.sectionTitle}>{t("projects.newProject")}</h3>
            <p className="text-muted-foreground mt-2 text-sm leading-6">
              {t("projects.startGuideNewProject")}
            </p>
          </button>
        </div>
        {error && <p className="text-destructive text-sm leading-6">{error}</p>}
        <p className={typography.metadata}>{t("projects.startGuideHint")}</p>
      </div>
    </div>
  );
}

function NewProjectForm() {
  const queryClient = useQueryClient();
  const { t } = useTranslation();
  const [name, setName] = useState("");
  const [rootPath, setRootPath] = useState("");
  const [isPickingFolder, setIsPickingFolder] = useState(false);
  const [pathPickerError, setPathPickerError] = useState<string | null>(null);

  const create = useMutation({
    mutationFn: () => createWorkspace(name, rootPath || ".", false),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: workspaceKeys.list() });
      setName("");
      setRootPath("");
    },
  });

  async function chooseProjectFolder() {
    setPathPickerError(null);
    setIsPickingFolder(true);
    try {
      const selection = await openDialog({
        directory: true,
        multiple: false,
        title: t("projects.chooseProjectFolder"),
      });
      const path = normalizeDirectorySelection(selection);
      if (path) setRootPath(path);
    } catch (error) {
      setPathPickerError(error instanceof Error ? error.message : String(error));
    } finally {
      setIsPickingFolder(false);
    }
  }

  return (
    <div className="mx-auto max-w-xl">
      <form
        className="agent-panel space-y-3 p-4"
        onSubmit={(e) => {
          e.preventDefault();
          create.mutate();
        }}
      >
        <h2 className={typography.sectionTitle}>{t("projects.newProject")}</h2>
        <Input
          placeholder={t("projects.projectName")}
          value={name}
          onChange={(e) => setName(e.target.value)}
          required
        />
        <div className="space-y-2">
          <div className="flex gap-2">
            <button
              type="button"
              className="border-input bg-background text-muted-foreground hover:bg-muted hover:text-foreground flex h-10 min-w-0 flex-1 items-center rounded-md border px-3 text-left text-sm transition-colors"
              onClick={chooseProjectFolder}
            >
              <span className="min-w-0 flex-1 truncate">
                {rootPath || t("projects.projectFolder")}
              </span>
            </button>
            <Button
              type="button"
              variant="outline"
              onClick={chooseProjectFolder}
              disabled={isPickingFolder}
              className="shrink-0"
            >
              <FolderOpen className="h-4 w-4" />
              {t("projects.chooseProjectFolder")}
            </Button>
          </div>
          {pathPickerError && <p className="text-destructive text-xs">{pathPickerError}</p>}
        </div>
        <Button type="submit" disabled={create.isPending} className="w-full">
          <Plus className="h-4 w-4" />
          {t("projects.newProject")}
        </Button>
      </form>
    </div>
  );
}
