import { createFileRoute, Link } from "@tanstack/react-router";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import {
  gitBranch,
  gitCommit,
  gitDiff,
  gitPush,
  gitStage,
  gitStatus,
  gitUnstage,
} from "@/lib/tauri-api";
import { asWorkspaceId } from "@/lib/schemas";
import { useMemo, useState } from "react";
import { useTranslation } from "@/lib/i18n-react";

export const Route = createFileRoute("/workspaces/$workspaceId/git/")({
  component: GitPage,
});

function GitPage() {
  const { t } = useTranslation();
  const { workspaceId: workspaceIdParam } = Route.useParams();
  const workspaceId = asWorkspaceId(workspaceIdParam);
  const [repoPath, setRepoPath] = useState("");
  const [selectedFiles, setSelectedFiles] = useState<Set<string>>(new Set());
  const [commitMessage, setCommitMessage] = useState("");
  const [branchName, setBranchName] = useState("");

  const { data: statusText, isLoading: statusLoading } = useQuery({
    queryKey: ["git", workspaceId, repoPath, "status"],
    queryFn: () => gitStatus(workspaceId, repoPath),
    enabled: !!repoPath,
  });

  const { data: diffText, isLoading: diffLoading } = useQuery({
    queryKey: ["git", workspaceId, repoPath, "diff"],
    queryFn: () => gitDiff(workspaceId, repoPath),
    enabled: !!repoPath,
  });

  const files = useMemo(() => parseStatusFiles(statusText ?? ""), [statusText]);

  const queryClient = useQueryClient();

  const stage = useMutation({
    mutationFn: () => gitStage(workspaceId, repoPath, Array.from(selectedFiles)),
    onSuccess: () => {
      setSelectedFiles(new Set());
      void queryClient.invalidateQueries({ queryKey: ["git", workspaceId, repoPath, "status"] });
      void queryClient.invalidateQueries({ queryKey: ["git", workspaceId, repoPath, "diff"] });
    },
  });

  const unstage = useMutation({
    mutationFn: () => gitUnstage(workspaceId, repoPath, Array.from(selectedFiles)),
    onSuccess: () => {
      setSelectedFiles(new Set());
      void queryClient.invalidateQueries({ queryKey: ["git", workspaceId, repoPath, "status"] });
    },
  });

  const commit = useMutation({
    mutationFn: () => gitCommit(workspaceId, repoPath, commitMessage),
    onSuccess: () => {
      setCommitMessage("");
      void queryClient.invalidateQueries({ queryKey: ["git", workspaceId, repoPath, "status"] });
      void queryClient.invalidateQueries({ queryKey: ["git", workspaceId, repoPath, "diff"] });
    },
  });

  const branch = useMutation({
    mutationFn: () => gitBranch(workspaceId, repoPath, branchName || undefined),
    onSuccess: () => {
      setBranchName("");
      void queryClient.invalidateQueries({ queryKey: ["git", workspaceId, repoPath, "status"] });
    },
  });

  const push = useMutation({
    mutationFn: () => gitPush(workspaceId, repoPath),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["git", workspaceId, repoPath, "status"] });
    },
  });

  function toggleFile(file: string) {
    setSelectedFiles((prev) => {
      const next = new Set(prev);
      if (next.has(file)) {
        next.delete(file);
      } else {
        next.add(file);
      }
      return next;
    });
  }

  return (
    <main className="container mx-auto max-w-4xl p-6">
      <div className="mb-4">
        <Link
          to="/workspaces/$workspaceId"
          params={{ workspaceId }}
          className="text-muted-foreground text-sm hover:underline"
        >
          ← {t("common.workspace")}
        </Link>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Git</CardTitle>
        </CardHeader>
        <CardContent className="space-y-6">
          <div className="flex gap-2">
            <Input
              placeholder={t("git.repositoryPath")}
              value={repoPath}
              onChange={(e) => setRepoPath(e.target.value)}
            />
          </div>

          <div className="space-y-2">
            <h3 className="text-sm font-medium">{t("git.status")}</h3>
            {statusLoading ? (
              <p className="text-muted-foreground text-sm">{t("git.loadingStatus")}</p>
            ) : statusText ? (
              <>
                {files.length ? (
                  <ul className="border-border max-h-60 space-y-1 overflow-y-auto rounded-md border p-2">
                    {files.map((file) => (
                      <li key={file} className="flex items-center gap-2 text-sm">
                        <input
                          type="checkbox"
                          checked={selectedFiles.has(file)}
                          onChange={() => toggleFile(file)}
                          className="h-4 w-4"
                        />
                        <span className="font-mono">{file}</span>
                      </li>
                    ))}
                  </ul>
                ) : (
                  <p className="text-muted-foreground text-sm">{t("git.noChanges")}</p>
                )}
                <pre className="bg-muted mt-2 rounded-md p-3 font-mono text-xs whitespace-pre-wrap">
                  {statusText}
                </pre>
              </>
            ) : (
              <p className="text-muted-foreground text-sm">
                {t("git.enterRepoStatus")}
              </p>
            )}
          </div>

          <div className="flex flex-wrap gap-2">
            <Button
              onClick={() => stage.mutate()}
              disabled={stage.isPending || selectedFiles.size === 0}
            >
              {t("git.stageSelected")}
            </Button>
            <Button
              variant="outline"
              onClick={() => unstage.mutate()}
              disabled={unstage.isPending || selectedFiles.size === 0}
            >
              {t("git.unstageSelected")}
            </Button>
          </div>

          <div className="space-y-2">
            <h3 className="text-sm font-medium">{t("git.diff")}</h3>
            {diffLoading ? (
              <p className="text-muted-foreground text-sm">{t("git.loadingDiff")}</p>
            ) : diffText ? (
              <pre className="bg-muted max-h-96 overflow-auto rounded-md p-3 font-mono text-xs whitespace-pre-wrap">
                {diffText}
              </pre>
            ) : (
              <p className="text-muted-foreground text-sm">{t("git.enterRepoDiff")}</p>
            )}
          </div>

          <div className="space-y-2">
            <h3 className="text-sm font-medium">{t("git.commit")}</h3>
            <Textarea
              placeholder={t("git.commitMessage")}
              value={commitMessage}
              onChange={(e) => setCommitMessage(e.target.value)}
              rows={2}
            />
            <Button
              onClick={() => commit.mutate()}
              disabled={commit.isPending || !commitMessage.trim()}
            >
              {t("git.commit")}
            </Button>
          </div>

          <div className="space-y-2">
            <h3 className="text-sm font-medium">{t("git.branch")}</h3>
            <div className="flex gap-2">
              <Input
                placeholder={t("git.newBranchName")}
                value={branchName}
                onChange={(e) => setBranchName(e.target.value)}
              />
              <Button onClick={() => branch.mutate()} disabled={branch.isPending}>
                {t("git.branch")}
              </Button>
            </div>
          </div>

          <div className="space-y-2">
            <h3 className="text-sm font-medium">{t("git.push")}</h3>
            <Button onClick={() => push.mutate()} disabled={push.isPending}>
              {t("git.push")}
            </Button>
          </div>
        </CardContent>
      </Card>
    </main>
  );
}

function parseStatusFiles(status: string): string[] {
  const files: string[] = [];
  for (const line of status.split("\n")) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith("On branch") || trimmed.startsWith("Your branch")) continue;
    // git status --short lines start with two status characters followed by a path.
    const match = trimmed.match(/^\s*[A-Z?][A-Z?]?\s+(.+)$/);
    if (match) {
      files.push(match[1]);
    }
  }
  return files;
}
