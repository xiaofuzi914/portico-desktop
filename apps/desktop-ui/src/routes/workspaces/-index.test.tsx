import { act, type ComponentType } from "react";
import { createRoot, type Root } from "react-dom/client";
import { describe, expect, it, vi } from "vitest";
import { I18nProvider } from "@/lib/i18n-react";
import * as WorkspacesModule from "./index";

interface ProjectStartGuideProps {
  onOpenExistingProject: () => void;
  onCreateNewProject: () => void;
  isOpeningExistingProject?: boolean;
}

const ProjectStartGuide = (
  WorkspacesModule as unknown as {
    ProjectStartGuide?: ComponentType<ProjectStartGuideProps>;
  }
).ProjectStartGuide;

function findButton(container: HTMLElement, label: string): HTMLButtonElement {
  const button = Array.from(container.querySelectorAll("button")).find((candidate) =>
    candidate.textContent?.includes(label),
  );
  if (!button) throw new Error(`Missing button: ${label}`);
  return button;
}

describe("ProjectStartGuide", () => {
  it("routes each start card click to the matching project action", async () => {
    expect(ProjectStartGuide).toBeTypeOf("function");
    if (!ProjectStartGuide) return;

    const openExistingProject = vi.fn();
    const createNewProject = vi.fn();
    const container = document.createElement("div");
    document.body.appendChild(container);
    let root: Root | null = null;

    try {
      await act(async () => {
        root = createRoot(container);
        root.render(
          <I18nProvider>
            <ProjectStartGuide
              onOpenExistingProject={openExistingProject}
              onCreateNewProject={createNewProject}
            />
          </I18nProvider>,
        );
      });

      await act(async () => {
        findButton(container, "Open existing project").dispatchEvent(
          new MouseEvent("click", { bubbles: true }),
        );
      });
      expect(openExistingProject).toHaveBeenCalledOnce();

      await act(async () => {
        findButton(container, "New project").dispatchEvent(
          new MouseEvent("click", { bubbles: true }),
        );
      });
      expect(createNewProject).toHaveBeenCalledOnce();
    } finally {
      await act(async () => {
        root?.unmount();
      });
      container.remove();
    }
  });
});
