import { createFileRoute } from "@tanstack/react-router";
import { CapabilitiesCenter } from "@/features/capabilities/capabilities-center";

export const Route = createFileRoute("/memory/")({
  component: MemoryCapabilitiesPage,
});

function MemoryCapabilitiesPage() {
  return <CapabilitiesCenter defaultTab="memory" />;
}
