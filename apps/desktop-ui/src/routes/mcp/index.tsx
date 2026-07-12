import { createFileRoute } from "@tanstack/react-router";
import { CapabilitiesCenter } from "@/features/capabilities/capabilities-center";

export const Route = createFileRoute("/mcp/")({
  component: McpPage,
});

function McpPage() {
  return <CapabilitiesCenter defaultTab="mcp" />;
}
