# Portico Agent Client UI Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Redesign Portico Desktop from a flat collection of feature pages into a production-grade local Agent client similar in workflow shape to Codex and Claude: project sidebar, thread-centered conversation, execution timeline, right-side inspector, artifacts, and configuration surfaces.

**Architecture:** Keep the current Rust/Tauri/backend domain architecture intact. The redesign is a frontend information-architecture and component-composition change that reuses existing `Workspace`, `Thread`, `AgentRun`, `RunEvent`, `Terminal`, `Git`, `Browser`, `Desktop`, `Plugins`, `Skills`, `Models`, `Notifications`, `BackgroundTasks`, and `Audit` APIs. Add frontend-only adapter/view-model helpers where aggregation is needed; do not introduce a parallel backend model unless a task explicitly calls for a small API extension.

**Tech Stack:** Tauri 2, React 19, TypeScript, TanStack Router, TanStack Query, Tailwind CSS 4, lucide-react, Vitest, existing Rust workspace crates.

---

## 1. Product Direction

Portico should feel like a local desktop Agent client, not a settings dashboard and not a testing console.

Primary user loop:

```text
Open app
  -> choose or create project/workspace
  -> select or create thread
  -> chat with agent
  -> watch execution and tool calls
  -> inspect files, terminal, browser, context, artifacts, audit
  -> approve, stop, continue, or review output
```

The first-class object is the **thread inside a project**. Models, plugins, skills, MCP, browser, desktop, automations, audit, and settings are supporting surfaces, not the main product surface.

## 2. Compatibility With Current Project

This plan is compatible with the current repository because the backend already exposes most of the needed domain concepts.

| New UI Surface | Existing API / Model | Compatibility |
| --- | --- | --- |
| Project sidebar | `listWorkspaces`, `createWorkspace`, `trustWorkspace` | Direct reuse |
| Thread list | `listThreads`, `createThread` | Direct reuse |
| Conversation main | `startRun`, `submitMessage`, `cancelRun`, `pauseRun`, `resumeRun`, `listRunEvents` | Direct reuse |
| Live execution | `useRuntimeEvents`, `RunEvent`, `RuntimeEvent` | Direct reuse |
| Right inspector: context | `inspectContext`, instructions, memory, RAG schemas | Direct reuse |
| Right inspector: files/diff | `gitStatus`, `gitDiff`, `gitStage`, `gitUnstage`, `gitCommit`, `gitBranch`, `gitPush` | Direct reuse |
| Right inspector: terminal | `createTerminal`, `executeTerminalCommand`, `readTerminalHistory` | Direct reuse |
| Right inspector: browser | `openBrowserWindow`, `browserUseAction`, `closeBrowserWindow`, browser schemas | Direct reuse |
| Right inspector: desktop | existing desktop route and Tauri API commands | Direct reuse |
| Artifacts | `Artifact`, `ArtifactPreview`, `artifact-preview.tsx` | Direct reuse, needs better placement |
| Inbox/background tasks | `listNotifications`, `listBackgroundTasks`, automations APIs | Direct reuse |
| Capabilities center | `listModels`, providers, plugins, skills, MCP APIs | Direct reuse |
| Audit/diagnostics | `listAuditLog`, diagnostics APIs | Direct reuse |

Do not remove the current routes. Reorganize their entry points and progressively move their most important views into the Agent shell.

## 3. Non-Goals

- Do not build a marketing landing page.
- Do not build a feature-testing dashboard as the primary home.
- Do not redesign the backend crate graph.
- Do not replace TanStack Router or TanStack Query.
- Do not introduce Redux, Zustand, or another global state library unless a later task proves local/query state is insufficient.
- Do not hardcode provider API keys, model secrets, file paths, or local usernames.
- Do not make the UI one-note purple/blue, gradient-heavy, or decorative.

## 4. UX Principles

- The app should open into a working Agent client, not a documentation page.
- The visual hierarchy is: project, thread, conversation, execution, inspector.
- Settings and capability management are available but visually subordinate.
- Tool calls should be readable as execution blocks in the conversation and inspectable in detail on the right.
- Long-running work should remain visible through an Inbox/Tasks area.
- The UI should support keyboard-heavy desktop usage and dense scanning.
- Every fixed-format area must have stable dimensions: sidebar width, header height, composer height, inspector width, scroll regions.
- Use lucide-react icons for navigation and tool buttons.
- Use text labels where navigation meaning matters; use icon buttons for compact commands such as stop, refresh, send, split view, close, collapse.

## 5. Target Information Architecture

```text
Portico Desktop
├─ App Shell
│  ├─ Left Sidebar
│  │  ├─ Brand / active workspace switcher
│  │  ├─ Project list
│  │  ├─ Thread list for selected project
│  │  ├─ Inbox / background tasks
│  │  └─ Settings / capabilities entry
│  ├─ Main Thread Area
│  │  ├─ Thread header: workspace, thread, branch, model, run status
│  │  ├─ Conversation timeline
│  │  ├─ Inline tool call / approval / artifact blocks
│  │  └─ Composer
│  └─ Right Inspector
│     ├─ Context
│     ├─ Files / Diff
│     ├─ Terminal
│     ├─ Browser
│     ├─ Desktop
│     ├─ Artifacts
│     └─ Audit
├─ Capabilities Center
│  ├─ Models / providers
│  ├─ Plugins
│  ├─ Skills
│  └─ MCP servers
└─ Operations
   ├─ Automations
   ├─ Notifications
   ├─ Background tasks
   └─ Diagnostics / audit
```

## 6. Proposed File Structure

Create focused frontend modules instead of stuffing the new shell into `routes/__root.tsx`.

```text
apps/desktop-ui/src/components/app-shell/
├─ app-shell.tsx
├─ app-sidebar.tsx
├─ app-topbar.tsx
├─ sidebar-projects.tsx
├─ sidebar-threads.tsx
├─ sidebar-inbox.tsx
└─ inspector-shell.tsx

apps/desktop-ui/src/features/agent-client/
├─ agent-client-page.tsx
├─ conversation-timeline.tsx
├─ conversation-event-block.tsx
├─ conversation-composer.tsx
├─ thread-header.tsx
├─ run-controls.tsx
├─ event-view-models.ts
└─ event-view-models.test.ts

apps/desktop-ui/src/features/inspector/
├─ context-panel.tsx
├─ files-panel.tsx
├─ terminal-panel.tsx
├─ browser-panel.tsx
├─ desktop-panel.tsx
├─ artifacts-panel.tsx
├─ audit-panel.tsx
├─ inspector-tabs.tsx
└─ inspector-state.ts

apps/desktop-ui/src/features/capabilities/
├─ capabilities-center.tsx
├─ model-capabilities-panel.tsx
├─ plugin-capabilities-panel.tsx
├─ skill-capabilities-panel.tsx
└─ mcp-capabilities-panel.tsx

apps/desktop-ui/src/features/operations/
├─ operations-center.tsx
├─ background-task-list.tsx
├─ notification-inbox.tsx
└─ automation-summary.tsx
```

Modify existing route files to compose these feature modules:

```text
apps/desktop-ui/src/routes/__root.tsx
apps/desktop-ui/src/routes/index.tsx
apps/desktop-ui/src/routes/workspaces/index.tsx
apps/desktop-ui/src/routes/workspaces/$workspaceId/index.tsx
apps/desktop-ui/src/routes/workspaces/$workspaceId/threads/$threadId/index.tsx
apps/desktop-ui/src/routes/models/index.tsx
apps/desktop-ui/src/routes/plugins/index.tsx
apps/desktop-ui/src/routes/skills/index.tsx
apps/desktop-ui/src/routes/automations/index.tsx
apps/desktop-ui/src/routes/audit/index.tsx
apps/desktop-ui/src/routes/browser/index.tsx
apps/desktop-ui/src/routes/desktop/index.tsx
apps/desktop-ui/src/routes/settings/index.tsx
apps/desktop-ui/src/styles.css
```

Keep low-level Tauri calls in:

```text
apps/desktop-ui/src/lib/tauri-api.ts
apps/desktop-ui/src/lib/tauri-events.ts
apps/desktop-ui/src/lib/schemas.ts
```

Only add to `tauri-api.ts` when a task requires a missing aggregation command. Prefer composing existing query calls in feature modules first.

## 7. Architecture Control Rules

1. **Backend compatibility first:** Every screen must start from existing Tauri APIs. Add backend commands only after proving existing APIs cannot represent the UI state.
2. **Thread-centric navigation:** Deep links must still work for `/workspaces/$workspaceId/threads/$threadId`.
3. **No route deletion:** Existing routes stay available for compatibility, but their UI can be redesigned.
4. **No hidden mutation:** UI state updates should create new arrays/objects instead of mutating existing data.
5. **Query keys are stable:** Use query keys such as `["workspaces"]`, `["workspaces", workspaceId, "threads"]`, `["runs", runId, "events"]`, `["background-tasks", workspaceId]`.
6. **Secrets stay out of UI state:** Provider API keys must remain references only. Never render or persist raw secrets.
7. **Inspector panels are independent:** A broken Browser panel must not break Files, Context, or Terminal panels.
8. **Conversation remains usable offline from optional panels:** If models/plugins/audit fail to load, the main conversation layout should still render with a clear inline error.
9. **Desktop ergonomics:** Use fixed sidebar/inspector widths with responsive collapse behavior under narrow widths.
10. **Testing discipline:** Add unit tests for event mapping and shell state logic before implementing UI behavior.

## 8. Data Flow

### 8.1 Workspace and Thread Selection

```text
AppShell
  -> listWorkspaces()
  -> selected workspace from route params or first available workspace
  -> listThreads(selectedWorkspaceId)
  -> selected thread from route params or no thread selected state
```

Route behavior:

- `/` should render the Agent client shell.
- If no workspace exists, show an empty project state in the main area.
- If workspace exists but no thread is selected, show a project overview and thread creation affordance.
- If both workspace and thread exist, render the conversation page.
- `/workspaces/$workspaceId/threads/$threadId` should render the same Agent client layout with explicit selected IDs.

### 8.2 Run and Event Flow

```text
ThreadHeader
  -> shows active workspace/thread/run status

ConversationTimeline
  -> listRunEvents(activeRunId)
  -> useRuntimeEvents(activeRunId)
  -> merge persisted and live events
  -> map raw events to display blocks

Composer
  -> startRun(workspaceId, threadId) when no active run exists
  -> submitMessage(activeRunId, content)
  -> cancelRun/pauseRun/resumeRun from run controls
```

### 8.3 Inspector Flow

```text
InspectorShell
  -> active tab stored in URL search param or component state
  -> ContextPanel uses inspectContext()
  -> FilesPanel uses gitStatus/gitDiff
  -> TerminalPanel uses createTerminal/executeTerminalCommand/readTerminalHistory
  -> BrowserPanel reuses browser APIs
  -> DesktopPanel reuses desktop APIs
  -> ArtifactsPanel uses artifact schemas/components
  -> AuditPanel uses listAuditLog(workspaceId, threadId, runId)
```

## 9. Implementation Tasks

### Task 1: Add Frontend Event View Models

**Files:**
- Create: `apps/desktop-ui/src/features/agent-client/event-view-models.ts`
- Create: `apps/desktop-ui/src/features/agent-client/event-view-models.test.ts`

- [ ] **Step 1: Write failing tests for raw run-event mapping**

Create `apps/desktop-ui/src/features/agent-client/event-view-models.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { mapRunEventToBlock, mergeRunEvents } from "./event-view-models";
import type { RunEvent, RuntimeEvent } from "@/lib/schemas";

const baseEvent: RunEvent = {
  id: 1,
  run_id: "run-1" as RunEvent["run_id"],
  thread_id: "thread-1" as RunEvent["thread_id"],
  sequence: 1,
  event_type: "Message",
  payload: { role: "assistant", content: "Hello" },
  created_at: "2026-07-07T00:00:00.000Z",
};

describe("event view models", () => {
  it("maps message events into conversation blocks", () => {
    expect(mapRunEventToBlock(baseEvent)).toEqual({
      id: "event-1",
      sequence: 1,
      kind: "message",
      title: "Assistant",
      body: "Hello",
      tone: "default",
      createdAt: "2026-07-07T00:00:00.000Z",
      raw: baseEvent,
    });
  });

  it("maps unknown events into diagnostic blocks", () => {
    const event = { ...baseEvent, id: 2, event_type: "SomethingNew", payload: { value: 42 } };

    expect(mapRunEventToBlock(event)).toMatchObject({
      id: "event-2",
      kind: "diagnostic",
      title: "SomethingNew",
      tone: "muted",
    });
  });

  it("merges persisted and live events in sequence order without mutating inputs", () => {
    const persisted = [baseEvent];
    const live: RuntimeEvent[] = [
      {
        kind: "RunCompleted",
        run_id: "run-1" as RuntimeEvent["run_id"],
        thread_id: "thread-1" as RuntimeEvent["thread_id"],
        timestamp: "2026-07-07T00:00:01.000Z",
      },
    ];

    const merged = mergeRunEvents(persisted, live);

    expect(merged).toHaveLength(2);
    expect(merged[0]?.id).toBe(1);
    expect(merged[1]?.event_type).toBe("RunCompleted");
    expect(persisted).toHaveLength(1);
  });
});
```

- [ ] **Step 2: Run test and verify failure**

Run:

```bash
pnpm --dir apps/desktop-ui test -- event-view-models
```

Expected: FAIL because `event-view-models.ts` does not exist.

- [ ] **Step 3: Implement view-model helpers**

Create `apps/desktop-ui/src/features/agent-client/event-view-models.ts`:

```ts
import type { RunEvent, RuntimeEvent } from "@/lib/schemas";

export type ConversationBlockKind =
  | "message"
  | "tool"
  | "approval"
  | "artifact"
  | "status"
  | "error"
  | "diagnostic";

export type ConversationBlockTone = "default" | "muted" | "success" | "warning" | "danger";

export interface ConversationBlock {
  id: string;
  sequence: number;
  kind: ConversationBlockKind;
  title: string;
  body: string;
  tone: ConversationBlockTone;
  createdAt: string;
  raw: RunEvent;
}

interface MessagePayload {
  role?: string;
  content?: string;
}

function payloadRecord(payload: unknown): Record<string, unknown> {
  return typeof payload === "object" && payload !== null ? (payload as Record<string, unknown>) : {};
}

function payloadText(payload: unknown): string {
  if (typeof payload === "string") return payload;
  const record = payloadRecord(payload);
  if (typeof record.content === "string") return record.content;
  if (typeof record.message === "string") return record.message;
  return JSON.stringify(payload, null, 2);
}

function titleCase(value: string): string {
  if (!value) return "Event";
  return value.slice(0, 1).toUpperCase() + value.slice(1);
}

export function mapRunEventToBlock(event: RunEvent): ConversationBlock {
  const payload = payloadRecord(event.payload) as MessagePayload;
  const eventType = event.event_type;

  if (eventType.toLowerCase().includes("error") || eventType === "RunFailed") {
    return {
      id: `event-${event.id}`,
      sequence: event.sequence,
      kind: "error",
      title: "Error",
      body: payloadText(event.payload),
      tone: "danger",
      createdAt: event.created_at,
      raw: event,
    };
  }

  if (eventType.toLowerCase().includes("tool")) {
    return {
      id: `event-${event.id}`,
      sequence: event.sequence,
      kind: "tool",
      title: "Tool Call",
      body: payloadText(event.payload),
      tone: "muted",
      createdAt: event.created_at,
      raw: event,
    };
  }

  if (eventType.toLowerCase().includes("approval")) {
    return {
      id: `event-${event.id}`,
      sequence: event.sequence,
      kind: "approval",
      title: "Approval Required",
      body: payloadText(event.payload),
      tone: "warning",
      createdAt: event.created_at,
      raw: event,
    };
  }

  if (eventType.toLowerCase().includes("artifact")) {
    return {
      id: `event-${event.id}`,
      sequence: event.sequence,
      kind: "artifact",
      title: "Artifact",
      body: payloadText(event.payload),
      tone: "success",
      createdAt: event.created_at,
      raw: event,
    };
  }

  if (eventType === "Message") {
    const role = typeof payload.role === "string" ? payload.role : "assistant";
    return {
      id: `event-${event.id}`,
      sequence: event.sequence,
      kind: "message",
      title: titleCase(role),
      body: payloadText(event.payload),
      tone: "default",
      createdAt: event.created_at,
      raw: event,
    };
  }

  if (eventType.startsWith("Run")) {
    return {
      id: `event-${event.id}`,
      sequence: event.sequence,
      kind: "status",
      title: eventType,
      body: payloadText(event.payload),
      tone: eventType === "RunCompleted" ? "success" : "muted",
      createdAt: event.created_at,
      raw: event,
    };
  }

  return {
    id: `event-${event.id}`,
    sequence: event.sequence,
    kind: "diagnostic",
    title: eventType,
    body: payloadText(event.payload),
    tone: "muted",
    createdAt: event.created_at,
    raw: event,
  };
}

export function runtimeEventToRunEvent(event: RuntimeEvent, index: number): RunEvent {
  return {
    id: -index,
    run_id: event.run_id,
    thread_id: event.thread_id,
    sequence: index,
    event_type: event.kind,
    payload: event,
    created_at: event.timestamp,
  };
}

export function mergeRunEvents(persisted: RunEvent[], live: RuntimeEvent[]): RunEvent[] {
  const liveOffset = persisted.length + 1;
  const liveEvents = live.map((event, index) => runtimeEventToRunEvent(event, liveOffset + index));
  return [...persisted, ...liveEvents].sort((a, b) => a.sequence - b.sequence);
}
```

- [ ] **Step 4: Verify tests pass**

Run:

```bash
pnpm --dir apps/desktop-ui test -- event-view-models
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/desktop-ui/src/features/agent-client/event-view-models.ts apps/desktop-ui/src/features/agent-client/event-view-models.test.ts
git commit -m "test: add agent event view models"
```

### Task 2: Build the Agent Client App Shell

**Files:**
- Create: `apps/desktop-ui/src/components/app-shell/app-shell.tsx`
- Create: `apps/desktop-ui/src/components/app-shell/app-sidebar.tsx`
- Create: `apps/desktop-ui/src/components/app-shell/sidebar-projects.tsx`
- Create: `apps/desktop-ui/src/components/app-shell/sidebar-threads.tsx`
- Create: `apps/desktop-ui/src/components/app-shell/sidebar-inbox.tsx`
- Create: `apps/desktop-ui/src/components/app-shell/app-topbar.tsx`
- Create: `apps/desktop-ui/src/components/app-shell/inspector-shell.tsx`
- Modify: `apps/desktop-ui/src/routes/__root.tsx`

- [ ] **Step 1: Replace the horizontal nav concept with an app shell**

`routes/__root.tsx` should become a thin wrapper:

```tsx
import { Outlet, createRootRoute } from "@tanstack/react-router";
import { AppShell } from "@/components/app-shell/app-shell";

export const Route = createRootRoute({
  component: RootComponent,
});

function RootComponent() {
  return (
    <AppShell>
      <Outlet />
    </AppShell>
  );
}
```

- [ ] **Step 2: Create shell layout with stable regions**

`app-shell.tsx` responsibilities:

- Own the three-column desktop frame.
- Render `AppSidebar`.
- Render `AppTopbar`.
- Render the route outlet area passed as `children`.
- Keep `NotificationCenter` visible but subordinate.

Suggested layout:

```tsx
import type { ReactNode } from "react";
import { NotificationCenter } from "@/components/notifications/notification-center";
import { AppSidebar } from "./app-sidebar";
import { AppTopbar } from "./app-topbar";

export function AppShell({ children }: { children: ReactNode }) {
  return (
    <div className="flex h-screen min-h-screen overflow-hidden bg-background text-foreground">
      <AppSidebar />
      <div className="flex min-w-0 flex-1 flex-col">
        <header className="flex h-12 shrink-0 items-center justify-between border-b bg-background px-4">
          <AppTopbar />
          <NotificationCenter />
        </header>
        <div className="min-h-0 flex-1 overflow-hidden">{children}</div>
      </div>
    </div>
  );
}
```

- [ ] **Step 3: Sidebar should expose project/thread/task navigation**

`app-sidebar.tsx` responsibilities:

- Render brand/name.
- Render project list.
- Render thread list for selected workspace when route provides one.
- Render inbox/background task summary.
- Render secondary links to capabilities and operations.

Use lucide icons: `MessageSquare`, `Folder`, `Inbox`, `Settings`, `Puzzle`, `Bot`, `History`, `Monitor`.

- [ ] **Step 4: Keep existing routes reachable**

Sidebar links must include:

```text
/workspaces
/models
/plugins
/skills
/automations
/audit
/browser
/desktop
/settings
```

But visually group them as:

```text
Projects
Threads
Inbox
Capabilities
Operations
Native Tools
Settings
```

- [ ] **Step 5: Verify app renders**

Run:

```bash
pnpm --dir apps/desktop-ui typecheck
pnpm --dir apps/desktop-ui lint
```

Expected: both pass.

- [ ] **Step 6: Commit**

```bash
git add apps/desktop-ui/src/components/app-shell apps/desktop-ui/src/routes/__root.tsx
git commit -m "feat: add agent client app shell"
```

### Task 3: Make `/` the Agent Home Instead of Redirecting to Workspaces

**Files:**
- Modify: `apps/desktop-ui/src/routes/index.tsx`
- Create: `apps/desktop-ui/src/features/agent-client/agent-client-page.tsx`

- [ ] **Step 1: Remove redirect from root route**

Replace the current root redirect with a real component:

```tsx
import { createFileRoute } from "@tanstack/react-router";
import { AgentClientPage } from "@/features/agent-client/agent-client-page";

export const Route = createFileRoute("/")({
  component: AgentClientPage,
});
```

- [ ] **Step 2: Implement empty-state logic without becoming a dashboard**

`AgentClientPage` should:

- Load workspaces.
- If none exist, show project creation prompt.
- If workspace exists but no thread is selected, show the latest project and thread list.
- If route params are absent, do not auto-start a run.

Main empty state copy should be product-like:

```text
Select a project or create a new one to start a thread.
```

Do not use copy such as:

```text
Test the model integration
Validate browser automation
```

- [ ] **Step 3: Verify**

Run:

```bash
pnpm --dir apps/desktop-ui typecheck
pnpm --dir apps/desktop-ui test
```

Expected: typecheck and tests pass.

- [ ] **Step 4: Commit**

```bash
git add apps/desktop-ui/src/routes/index.tsx apps/desktop-ui/src/features/agent-client/agent-client-page.tsx
git commit -m "feat: make root route an agent client home"
```

### Task 4: Redesign the Thread Page as the Main Conversation Surface

**Files:**
- Modify: `apps/desktop-ui/src/routes/workspaces/$workspaceId/threads/$threadId/index.tsx`
- Create: `apps/desktop-ui/src/features/agent-client/thread-header.tsx`
- Create: `apps/desktop-ui/src/features/agent-client/conversation-timeline.tsx`
- Create: `apps/desktop-ui/src/features/agent-client/conversation-event-block.tsx`
- Create: `apps/desktop-ui/src/features/agent-client/conversation-composer.tsx`
- Create: `apps/desktop-ui/src/features/agent-client/run-controls.tsx`

- [ ] **Step 1: Extract current thread-page panels**

Move the existing inline panels out of the route file:

- Conversation rendering -> `conversation-timeline.tsx`
- Message form -> `conversation-composer.tsx`
- Start/cancel/pause/resume buttons -> `run-controls.tsx`
- Title/status area -> `thread-header.tsx`

The route should become orchestration glue:

```tsx
export const Route = createFileRoute("/workspaces/$workspaceId/threads/$threadId/")({
  component: ThreadPage,
});
```

It should pass `workspaceId`, `threadId`, `runId`, `events`, and callbacks into feature components.

- [ ] **Step 2: Conversation timeline requirements**

`ConversationTimeline` must:

- Merge persisted and live events with `mergeRunEvents`.
- Map events with `mapRunEventToBlock`.
- Render message, tool, approval, artifact, status, and error blocks distinctly.
- Keep one vertical scroll area.
- Avoid nested cards.

- [ ] **Step 3: Composer requirements**

`ConversationComposer` must:

- Use a textarea, not a single-line input.
- Submit with button click.
- Disable submit when no active run exists unless the component owns an option to start a run first.
- Show stop/pause/resume controls near the composer.
- Never resize the whole layout while typing.

- [ ] **Step 4: Header requirements**

`ThreadHeader` must show:

- Workspace name.
- Thread title.
- Run status.
- Active model/provider if available from current settings.
- Branch/worktree indicator if available.

- [ ] **Step 5: Verify**

Run:

```bash
pnpm --dir apps/desktop-ui typecheck
pnpm --dir apps/desktop-ui lint
pnpm --dir apps/desktop-ui test
```

Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add apps/desktop-ui/src/routes/workspaces/\$workspaceId/threads/\$threadId/index.tsx apps/desktop-ui/src/features/agent-client
git commit -m "feat: redesign thread page as agent conversation"
```

### Task 5: Build the Right Inspector

**Files:**
- Create: `apps/desktop-ui/src/features/inspector/inspector-tabs.tsx`
- Create: `apps/desktop-ui/src/features/inspector/inspector-state.ts`
- Create: `apps/desktop-ui/src/features/inspector/context-panel.tsx`
- Create: `apps/desktop-ui/src/features/inspector/files-panel.tsx`
- Create: `apps/desktop-ui/src/features/inspector/terminal-panel.tsx`
- Create: `apps/desktop-ui/src/features/inspector/browser-panel.tsx`
- Create: `apps/desktop-ui/src/features/inspector/desktop-panel.tsx`
- Create: `apps/desktop-ui/src/features/inspector/artifacts-panel.tsx`
- Create: `apps/desktop-ui/src/features/inspector/audit-panel.tsx`

- [ ] **Step 1: Define inspector tabs**

`inspector-state.ts`:

```ts
export const inspectorTabs = [
  "context",
  "files",
  "terminal",
  "browser",
  "desktop",
  "artifacts",
  "audit",
] as const;

export type InspectorTab = (typeof inspectorTabs)[number];

export function isInspectorTab(value: string): value is InspectorTab {
  return inspectorTabs.includes(value as InspectorTab);
}
```

- [ ] **Step 2: Create accessible tab controls**

`InspectorTabs` should render icon+label controls for the seven tabs. Use stable button widths or a scrollable tab strip.

- [ ] **Step 3: Context panel**

Reuse `inspectContext(runId, threadId, workspaceId, workspace.root_path, query)` from current thread page behavior. Show:

- loaded instruction files
- memory items
- RAG chunks
- estimated tokens
- privacy flags

- [ ] **Step 4: Files panel**

Use:

```ts
gitStatus(workspaceId, workspace.root_path)
gitDiff(workspaceId, workspace.root_path)
```

Show status first, diff second. Add refresh. Do not stage/commit automatically.

- [ ] **Step 5: Terminal panel**

Reuse existing terminal flow:

```ts
createTerminal(threadId)
executeTerminalCommand(id, command, cwd)
readTerminalHistory(id)
```

Keep command execution explicit.

- [ ] **Step 6: Browser and desktop panels**

Move the core interaction pieces from the existing `/browser` and `/desktop` routes into reusable panels. Keep the full routes as expanded management pages.

- [ ] **Step 7: Artifacts panel**

Reuse `components/artifact/artifact-preview.tsx`. If artifact listing API is insufficient, render artifacts embedded in run events first and record a backend follow-up.

- [ ] **Step 8: Audit panel**

Use `listAuditLog(workspaceId, threadId, runId)` when IDs are available.

- [ ] **Step 9: Verify**

Run:

```bash
pnpm --dir apps/desktop-ui typecheck
pnpm --dir apps/desktop-ui lint
pnpm --dir apps/desktop-ui test
```

Expected: all pass.

- [ ] **Step 10: Commit**

```bash
git add apps/desktop-ui/src/features/inspector
git commit -m "feat: add thread inspector panels"
```

### Task 6: Reframe Workspaces as Projects

**Files:**
- Modify: `apps/desktop-ui/src/routes/workspaces/index.tsx`
- Modify: `apps/desktop-ui/src/routes/workspaces/$workspaceId/index.tsx`

- [ ] **Step 1: Rename visible concept from Workspace to Project where appropriate**

Backend remains `Workspace`. UI text should use `Project` for user-facing navigation unless the page is explicitly about filesystem permissions.

Examples:

```text
Workspaces -> Projects
Create workspace -> New project
Trusted workspace -> Trusted project
Root path -> Project folder
```

- [ ] **Step 2: Project detail page**

The project detail page should show:

- project folder and trust state
- threads
- current allowed read/write paths
- memory/context entry
- git entry
- automation entry

It should not look like a generic settings form.

- [ ] **Step 3: Verify**

Run:

```bash
pnpm --dir apps/desktop-ui typecheck
pnpm --dir apps/desktop-ui lint
```

Expected: both pass.

- [ ] **Step 4: Commit**

```bash
git add apps/desktop-ui/src/routes/workspaces/index.tsx apps/desktop-ui/src/routes/workspaces/\$workspaceId/index.tsx
git commit -m "feat: reframe workspaces as projects"
```

### Task 7: Build Capabilities Center

**Files:**
- Create: `apps/desktop-ui/src/features/capabilities/capabilities-center.tsx`
- Create: `apps/desktop-ui/src/features/capabilities/model-capabilities-panel.tsx`
- Create: `apps/desktop-ui/src/features/capabilities/plugin-capabilities-panel.tsx`
- Create: `apps/desktop-ui/src/features/capabilities/skill-capabilities-panel.tsx`
- Create: `apps/desktop-ui/src/features/capabilities/mcp-capabilities-panel.tsx`
- Modify: `apps/desktop-ui/src/routes/models/index.tsx`
- Modify: `apps/desktop-ui/src/routes/plugins/index.tsx`
- Modify: `apps/desktop-ui/src/routes/skills/index.tsx`
- Modify: `apps/desktop-ui/src/routes/settings/index.tsx`

- [ ] **Step 1: Group model, plugin, skill, and MCP management as capabilities**

These pages should feel like one configuration area with consistent layout and status presentation.

- [ ] **Step 2: Model panel requirements**

Show:

- provider display name
- base URL
- enabled status
- model list
- capability flags: tools, streaming, vision, JSON schema, embeddings, context tokens
- health/test action if existing API supports it

- [ ] **Step 3: Plugin and skill panel requirements**

Show:

- enabled/available state
- required tools
- permissions
- trigger description
- manual invoke only where current API supports it

- [ ] **Step 4: MCP panel requirements**

Show registered MCP servers if current APIs expose them. If route/API exists but UI is thin, keep the panel read-only first.

- [ ] **Step 5: Verify**

Run:

```bash
pnpm --dir apps/desktop-ui typecheck
pnpm --dir apps/desktop-ui lint
pnpm --dir apps/desktop-ui test
```

Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add apps/desktop-ui/src/features/capabilities apps/desktop-ui/src/routes/models/index.tsx apps/desktop-ui/src/routes/plugins/index.tsx apps/desktop-ui/src/routes/skills/index.tsx apps/desktop-ui/src/routes/settings/index.tsx
git commit -m "feat: organize capabilities center"
```

### Task 8: Build Operations Center

**Files:**
- Create: `apps/desktop-ui/src/features/operations/operations-center.tsx`
- Create: `apps/desktop-ui/src/features/operations/background-task-list.tsx`
- Create: `apps/desktop-ui/src/features/operations/notification-inbox.tsx`
- Create: `apps/desktop-ui/src/features/operations/automation-summary.tsx`
- Modify: `apps/desktop-ui/src/routes/automations/index.tsx`
- Modify: `apps/desktop-ui/src/routes/audit/index.tsx`

- [ ] **Step 1: Make background work visible**

The operations area should expose:

- unread notifications
- running/queued/completed background tasks
- automations
- audit filters

- [ ] **Step 2: Preserve full automation management**

Keep current create/edit/delete/run-now automation functionality. Improve layout only.

- [ ] **Step 3: Audit page requirements**

Audit should be a diagnostic log with filters for workspace/project, thread, run, event category, and time if supported by current data.

- [ ] **Step 4: Verify**

Run:

```bash
pnpm --dir apps/desktop-ui typecheck
pnpm --dir apps/desktop-ui lint
pnpm --dir apps/desktop-ui test
```

Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add apps/desktop-ui/src/features/operations apps/desktop-ui/src/routes/automations/index.tsx apps/desktop-ui/src/routes/audit/index.tsx
git commit -m "feat: add operations center"
```

### Task 9: Visual System and Responsive Behavior

**Files:**
- Modify: `apps/desktop-ui/src/styles.css`
- Modify as needed: `apps/desktop-ui/src/components/ui/button.tsx`
- Modify as needed: `apps/desktop-ui/src/components/ui/card.tsx`

- [ ] **Step 1: Define production desktop layout tokens**

Add CSS variables/classes for:

```css
--sidebar-width: 280px;
--inspector-width: 380px;
--topbar-height: 48px;
--composer-min-height: 112px;
```

- [ ] **Step 2: Use a restrained multi-neutral palette**

Keep the app quiet and legible. Avoid decorative gradients. Use semantic colors for:

- active route
- muted text
- danger/error
- warning/approval
- success/completed
- border/separators

- [ ] **Step 3: Responsive collapse**

Under tablet/mobile width:

- sidebar can collapse above content
- inspector can become a drawer or stack below conversation
- composer remains visible
- no text overlap

- [ ] **Step 4: Verify visual layout manually**

Run:

```bash
pnpm --dir apps/desktop-ui dev
```

Open local Vite URL and inspect:

- desktop width around 1440px
- laptop width around 1280px
- narrow width around 390px

Expected:

- conversation remains primary
- sidebar and inspector do not overlap
- text does not overflow buttons
- no blank primary regions

- [ ] **Step 5: Commit**

```bash
git add apps/desktop-ui/src/styles.css apps/desktop-ui/src/components/ui/button.tsx apps/desktop-ui/src/components/ui/card.tsx
git commit -m "style: refine desktop agent client visual system"
```

### Task 10: Final Verification and Production Gate

**Files:**
- No new files unless fixes are required.

- [ ] **Step 1: Run frontend checks**

```bash
pnpm --dir apps/desktop-ui typecheck
pnpm --dir apps/desktop-ui lint
pnpm --dir apps/desktop-ui test
pnpm --dir apps/desktop-ui build
```

Expected: all pass.

- [ ] **Step 2: Run Rust checks**

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace
```

Expected: all pass.

If `cargo fmt --check` fails because of pre-existing formatting issues, run:

```bash
cargo fmt
cargo fmt --check
```

Then rerun clippy/tests/build.

- [ ] **Step 3: Build Tauri app**

```bash
pnpm tauri build
```

Expected: app bundle and dmg build successfully under `target/release/bundle/`.

- [ ] **Step 4: Manual acceptance checklist**

Verify:

- App opens to Agent client shell, not old horizontal nav.
- Sidebar shows projects and threads.
- Creating/selecting a project works.
- Creating/selecting a thread works.
- Thread page shows conversation timeline and composer.
- Starting a run works when model/provider config is valid.
- Submitting a message works.
- Cancelling/pausing/resuming a run works if backend supports the action.
- Live events appear in the timeline.
- Right inspector tabs render independently.
- Files/diff panel can show git status/diff for a repo workspace.
- Terminal panel can run a harmless command such as `pwd`.
- Context panel can inspect current run context after a run exists.
- Browser/Desktop panels do not crash the shell when no workspace is selected.
- Capabilities pages remain reachable.
- Automations and audit pages remain reachable.
- No raw API secrets are displayed.
- No primary text overlaps or spills out of buttons at 390px width.

- [ ] **Step 5: Commit final fixes**

```bash
git status --short
git add apps/desktop-ui crates Cargo.toml Cargo.lock
git commit -m "feat: redesign desktop UI as agent client"
```

## 10. Backend Follow-Ups After UI Shell Lands

These are not required for the first UI pass, but they are needed for a Codex/Claude-level production client.

### 10.1 Run Options

Current:

```ts
startRun(workspaceId, threadId)
```

Future:

```ts
startRun({
  workspaceId,
  threadId,
  providerId,
  modelId,
  approvalPolicy,
  sandboxMode,
  reasoningEffort,
  instructionProfileId,
})
```

Reason: Codex/Claude-style clients need per-thread/per-run control over model, approvals, sandboxing, and instruction profile.

### 10.2 Message Persistence

The UI currently derives conversation content from run events. Production chat history should persist explicit user/assistant messages linked to a thread.

Suggested model:

```text
thread_messages
├─ id
├─ thread_id
├─ run_id nullable
├─ role
├─ content
├─ created_at
└─ metadata_json
```

### 10.3 Artifacts Listing

Artifacts should be listable by `workspaceId`, `threadId`, and `runId`, not only previewed ad hoc.

Suggested command:

```ts
listArtifacts(workspaceId, threadId, runId?: AgentRunId): Promise<ArtifactPreview[]>
```

### 10.4 Unified Thread Snapshot

For faster UI loading, consider one aggregation command:

```ts
getThreadSnapshot(workspaceId, threadId): Promise<{
  workspace: Workspace;
  thread: Thread;
  latestRun: AgentRun | null;
  recentEvents: RunEvent[];
  backgroundTasks: BackgroundTask[];
  notifications: Notification[];
}>
```

Only add this after the UI is stable and duplicate query waterfalls become measurable.

## 11. Definition of Done

The redesign is complete when:

- The app no longer feels like a horizontal menu of unrelated admin pages.
- Project/thread/conversation is the main product path.
- Tool execution and run state are visible inside the conversation flow.
- Context/files/terminal/browser/desktop/artifacts/audit are available as inspector panels.
- Existing route URLs continue to work.
- Existing backend APIs are reused wherever possible.
- Frontend typecheck, lint, tests, and build pass.
- Rust fmt, clippy, tests, and build pass.
- Tauri build passes.
- Manual acceptance checklist passes.

## 12. Implementation Order Summary

Recommended order for Kimi Code multi-agent execution:

1. Agent A: event view models and tests.
2. Agent B: app shell/sidebar/topbar.
3. Agent C: root route and project/thread home behavior.
4. Agent D: thread conversation refactor.
5. Agent E: right inspector panels.
6. Agent F: project/workspace route refinements.
7. Agent G: capabilities center.
8. Agent H: operations center.
9. Agent I: visual system and responsive QA.
10. Agent J: final verification and build gate.

Agents should not edit the same file concurrently. Merge order should follow the task order above, because later tasks depend on the shell and event view models.

## 13. Notes for Kimi Code

- Treat `apps/desktop-ui/src/lib/tauri-api.ts` as the source of truth for frontend/backend commands.
- Treat `apps/desktop-ui/src/lib/schemas.ts` as the source of truth for TypeScript domain types.
- Keep UI components small and feature-scoped.
- Prefer extracting reusable panels over rewriting current route logic from scratch.
- Avoid changing Rust unless a frontend requirement cannot be met through existing APIs.
- If backend change is unavoidable, add tests in the relevant Rust crate before implementing.
- If a task discovers a pre-existing failing Rust formatting or clippy issue, fix it in a separate commit before continuing UI work.
