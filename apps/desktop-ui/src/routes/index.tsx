import { createFileRoute } from "@tanstack/react-router";
import { z } from "zod";
import { AgentClientPage } from "@/features/agent-client/agent-client-page";

const searchSchema = z.object({
  workspaceId: z.string().optional(),
});

export const Route = createFileRoute("/")({
  component: AgentClientPage,
  validateSearch: searchSchema,
});
