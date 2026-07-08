import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { createTerminal, executeTerminalCommand, readTerminalHistory } from "@/lib/tauri-api";
import type { TerminalId, ThreadId } from "@/lib/schemas";
import { useTranslation } from "@/lib/i18n-react";
import { InlineError, PanelLoading } from "./panel-primitives";
import { terminalKeys } from "@/lib/query-keys";

interface TerminalPanelProps {
  threadId: ThreadId;
}

export function TerminalPanel({ threadId }: TerminalPanelProps) {
  const queryClient = useQueryClient();
  const { t } = useTranslation();
  const [terminalId, setTerminalId] = useState<TerminalId | undefined>();
  const [terminalError, setTerminalError] = useState<string | null>(null);
  const [command, setCommand] = useState("");
  const [cwd, setCwd] = useState("");

  useEffect(() => {
    let cancelled = false;
    void createTerminal(threadId)
      .then((id) => {
        if (!cancelled) setTerminalId(id);
      })
      .catch((err: unknown) => {
        if (!cancelled) setTerminalError(err instanceof Error ? err.message : String(err));
      });
    return () => {
      cancelled = true;
    };
  }, [threadId]);

  const { data: history, isLoading: loadingHistory } = useQuery({
    queryKey: terminalKeys.history(terminalId),
    queryFn: () => readTerminalHistory(terminalId!),
    enabled: !!terminalId,
    refetchInterval: 1000,
  });

  const execute = useMutation({
    mutationFn: () => {
      if (!terminalId) throw new Error(t("agent.starting"));
      return executeTerminalCommand(terminalId, command, cwd);
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: terminalKeys.history(terminalId),
      });
      setCommand("");
    },
  });

  return (
    <div className="flex h-full flex-col gap-3 overflow-auto p-3">
      {terminalError && <InlineError title={t("inspector.terminalError")} message={terminalError} />}
      {!terminalId && !terminalError && <PanelLoading />}
      {terminalId && (
        <>
          <div className="flex gap-2">
            <Input
              placeholder={t("inspector.command")}
              value={command}
              onChange={(e) => setCommand(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && command.trim()) execute.mutate();
              }}
              className="h-8 flex-1 text-xs"
            />
            <Input
              placeholder="cwd"
              value={cwd}
              onChange={(e) => setCwd(e.target.value)}
              className="h-8 w-28 text-xs"
            />
            <Button
              size="sm"
              className="h-8 text-xs"
              disabled={!command.trim() || execute.isPending}
              onClick={() => execute.mutate()}
            >
              {t("inspector.runCommand")}
            </Button>
          </div>
          {execute.error && (
            <InlineError title={t("inspector.executionFailed")} message={execute.error.message} />
          )}
          <div className="flex-1 overflow-auto rounded border bg-black p-2 font-mono text-xs text-green-400">
            {loadingHistory && (
              <p className="text-muted-foreground">{t("inspector.loadingHistory")}</p>
            )}
            {!loadingHistory &&
              (history?.length ? (
                history.map((line, index) => <div key={index}>{line}</div>)
              ) : (
                <p className="text-muted-foreground">{t("inspector.noOutput")}</p>
              ))}
          </div>
        </>
      )}
    </div>
  );
}


