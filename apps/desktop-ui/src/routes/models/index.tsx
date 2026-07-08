import { createFileRoute } from "@tanstack/react-router";
import { CapabilitiesCenter } from "@/features/capabilities/capabilities-center";

export const Route = createFileRoute("/models/")({
  component: ModelsPage,
});

function ModelsPage() {
  return <CapabilitiesCenter defaultTab="models" />;
}
