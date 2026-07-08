import { createFileRoute } from "@tanstack/react-router";
import { CapabilitiesCenter } from "@/features/capabilities/capabilities-center";

export const Route = createFileRoute("/skills/")({
  component: SkillsPage,
});

function SkillsPage() {
  return <CapabilitiesCenter defaultTab="skills" />;
}
