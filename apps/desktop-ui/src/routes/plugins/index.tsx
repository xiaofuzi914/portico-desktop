import { createFileRoute } from "@tanstack/react-router";
import { CapabilitiesCenter } from "@/features/capabilities/capabilities-center";

export const Route = createFileRoute("/plugins/")({
  component: PluginsPage,
});

function PluginsPage() {
  return <CapabilitiesCenter defaultTab="plugins" />;
}
