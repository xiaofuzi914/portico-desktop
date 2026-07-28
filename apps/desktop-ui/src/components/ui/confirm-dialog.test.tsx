import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "@/lib/i18n-react";
import { ConfirmDialog } from "./confirm-dialog";

interface Harness {
  container: HTMLElement;
  root: Root;
}

async function renderDialog(
  props: Partial<Parameters<typeof ConfirmDialog>[0]> = {},
): Promise<Harness> {
  const container = document.createElement("div");
  document.body.appendChild(container);
  const root = createRoot(container);
  await act(async () => {
    root.render(
      <I18nProvider>
        <ConfirmDialog
          open
          title="Delete session"
          description="This cannot be undone."
          onConfirm={() => {}}
          onCancel={() => {}}
          {...props}
        />
      </I18nProvider>,
    );
  });
  return { container, root };
}

function findButton(container: HTMLElement, label: string): HTMLButtonElement {
  const button = Array.from(container.querySelectorAll("button")).find((candidate) =>
    candidate.textContent?.includes(label),
  );
  if (!button) throw new Error(`Missing button: ${label}`);
  return button;
}

describe("ConfirmDialog", () => {
  let harness: Harness | null = null;

  afterEach(async () => {
    if (harness) {
      await act(async () => {
        harness?.root.unmount();
      });
      harness.container.remove();
      harness = null;
    }
  });

  it("renders nothing when closed", async () => {
    harness = await renderDialog({ open: false });
    expect(harness.container.querySelector('[role="dialog"]')).toBeNull();
  });

  it("renders the title, description and default labels", async () => {
    harness = await renderDialog();
    const dialog = harness.container.querySelector('[role="dialog"]');
    expect(dialog).not.toBeNull();
    expect(dialog?.getAttribute("aria-modal")).toBe("true");
    expect(harness.container.textContent).toContain("Delete session");
    expect(harness.container.textContent).toContain("This cannot be undone.");
    expect(findButton(harness.container, "Cancel")).toBeDefined();
    expect(findButton(harness.container, "Confirm")).toBeDefined();
  });

  it("focuses the cancel button when it opens", async () => {
    harness = await renderDialog();
    expect(document.activeElement).toBe(findButton(harness.container, "Cancel"));
  });

  it("calls onCancel when Escape is pressed", async () => {
    const onCancel = vi.fn();
    harness = await renderDialog({ onCancel });
    await act(async () => {
      document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    });
    expect(onCancel).toHaveBeenCalledOnce();
  });

  it("keeps Tab cycling inside the dialog", async () => {
    harness = await renderDialog();
    const cancel = findButton(harness.container, "Cancel");
    const confirm = findButton(harness.container, "Confirm");

    // Tabbing from the last focusable element wraps back to the first.
    confirm.focus();
    await act(async () => {
      document.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab" }));
    });
    expect(document.activeElement).toBe(cancel);

    // Shift+Tab from the first element wraps to the last.
    cancel.focus();
    await act(async () => {
      document.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", shiftKey: true }));
    });
    expect(document.activeElement).toBe(confirm);
  });

  it("routes confirm and cancel clicks to their handlers", async () => {
    const onConfirm = vi.fn();
    const onCancel = vi.fn();
    harness = await renderDialog({ onConfirm, onCancel });
    const { container } = harness;

    await act(async () => {
      findButton(container, "Confirm").dispatchEvent(new MouseEvent("click", { bubbles: true }));
    });
    expect(onConfirm).toHaveBeenCalledOnce();

    await act(async () => {
      findButton(container, "Cancel").dispatchEvent(new MouseEvent("click", { bubbles: true }));
    });
    expect(onCancel).toHaveBeenCalledOnce();
  });
});
