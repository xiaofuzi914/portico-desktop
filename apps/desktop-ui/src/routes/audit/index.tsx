import { createFileRoute } from "@tanstack/react-router";
import { OperationsCenter } from "@/features/operations/operations-center";

export const Route = createFileRoute("/audit/")({
  component: AuditPage,
});

function AuditPage() {
  return <OperationsCenter defaultTab="audit" />;
}
