import { createFileRoute } from "@tanstack/react-router";
import { OperationsCenter } from "@/features/operations/operations-center";

export const Route = createFileRoute("/automations/")({
  component: AutomationsPage,
});

function AutomationsPage() {
  return <OperationsCenter defaultTab="automations" />;
}
