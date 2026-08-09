import { useState } from "react";
import { Link, useParams } from "@tanstack/react-router";
import {
  ChevronRight,
  Folder,
  MessageSquare,
  PanelLeftClose,
  PanelLeftOpen,
  Settings,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { asWorkspaceId } from "@/lib/schemas";
import { useTranslation } from "@/lib/i18n-react";
import { cn } from "@/lib/utils";
import { SidebarProjectActions } from "./sidebar-project-actions";
import { SidebarProjects } from "./sidebar-projects";
import { SidebarThreads } from "./sidebar-threads";
import { PorticoMark } from "./portico-mark";
import { buildNavigationSections } from "./navigation-model";
import {
  COLLAPSED_SIDEBAR_WIDTH,
  DEFAULT_SIDEBAR_WIDTH,
  readSidebarCollapsed,
  writeSidebarCollapsed,
} from "./shell-layout-state";
import { usePendingCandidateCount } from "@/features/memory/memory-center";
import {
  SIDEBAR_LEAF_ACTIVE_CLASS,
  SIDEBAR_LEAF_CLASS,
  SIDEBAR_SECTION_TITLE_CLASS,
} from "./sidebar-density";

export function AppSidebar() {
  const params = useParams({ strict: false }) as { workspaceId?: string };
  const workspaceId = params.workspaceId ? asWorkspaceId(params.workspaceId) : undefined;
  const { t } = useTranslation();
  const navigationSections = buildNavigationSections();
  const [collapsed, setCollapsed] = useState(readSidebarCollapsed);

  function setSidebarCollapsed(next: boolean) {
    setCollapsed(next);
    writeSidebarCollapsed(next);
  }

  const width = collapsed ? COLLAPSED_SIDEBAR_WIDTH : DEFAULT_SIDEBAR_WIDTH;

  return (
    <aside
      className="bg-sidebar text-sidebar-foreground hidden h-full shrink-0 flex-col border-r transition-[width] duration-200 ease-out lg:flex"
      style={{ width }}
      data-collapsed={collapsed ? "true" : "false"}
    >
      <div
        className={cn(
          "flex h-[var(--topbar-height)] items-center border-b",
          collapsed ? "justify-center px-1" : "gap-2 px-3",
        )}
      >
        {collapsed ? (
          <button
            type="button"
            className="focus-visible:ring-ring flex h-7 w-7 items-center justify-center overflow-hidden rounded-md focus-visible:ring-2 focus-visible:outline-none"
            onClick={() => setSidebarCollapsed(false)}
            aria-label={t("sidebar.expand")}
            title={t("sidebar.expand")}
          >
            <PorticoMark className="h-7 w-7" />
          </button>
        ) : (
          <>
            <PorticoMark className="h-7 w-7" />
            <div className="min-w-0 flex-1">
              <Link to="/" className="block truncate text-sm font-semibold">
                {t("app.name")}
              </Link>
              <p className="text-muted-foreground truncate text-[11px]">{t("app.tagline")}</p>
            </div>
            <button
              type="button"
              className="text-muted-foreground hover:bg-sidebar-accent hover:text-foreground focus-visible:ring-ring flex h-7 w-7 shrink-0 items-center justify-center rounded-md focus-visible:ring-2 focus-visible:outline-none"
              onClick={() => setSidebarCollapsed(true)}
              aria-label={t("sidebar.collapse")}
              aria-expanded={!collapsed}
              title={t("sidebar.collapse")}
            >
              <PanelLeftClose className="h-4 w-4" />
            </button>
          </>
        )}
      </div>

      <div
        className={cn(
          "flex min-h-0 flex-1 flex-col",
          collapsed ? "items-center overflow-y-auto px-1.5 py-2" : "px-2.5 py-2",
        )}
      >
        {collapsed ? (
          <>
            <div className="flex w-full flex-col items-center gap-1">
              <SidebarProjectActions compact />
              <SidebarProjects activeWorkspaceId={workspaceId} compact />
            </div>
            <div className="mt-auto flex flex-col items-center gap-1 pt-3">
              <button
                type="button"
                className="text-muted-foreground hover:bg-sidebar-accent hover:text-foreground focus-visible:ring-ring flex h-8 w-8 items-center justify-center rounded-md focus-visible:ring-2 focus-visible:outline-none"
                onClick={() => setSidebarCollapsed(false)}
                aria-label={t("sidebar.expand")}
                title={t("sidebar.expand")}
              >
                <PanelLeftOpen className="h-4 w-4" />
              </button>
              <IconNavLink to="/settings" icon={Settings} label={t("common.settings")} />
            </div>
          </>
        ) : (
          /*
            Top: projects + threads (scroll when long).
            Bottom: capabilities / operations / settings — compact, not stretched by empty space.
          */
          <div className="flex min-h-0 flex-1 flex-col">
            <div className="min-h-0 flex-1 space-y-3 overflow-y-auto pb-2">
              <SidebarSection
                icon={Folder}
                title={t("nav.projects")}
                action={<SidebarProjectActions />}
              >
                <SidebarProjects activeWorkspaceId={workspaceId} />
              </SidebarSection>

              {workspaceId && (
                <SidebarSection icon={MessageSquare} title={t("nav.threads")}>
                  <SidebarThreads workspaceId={workspaceId} />
                </SidebarSection>
              )}
            </div>

            <div className="mt-auto shrink-0 space-y-0.5 border-t border-border/60 pt-2">
              {navigationSections.map((section) => (
                <CollapsibleLinkGroup
                  key={section.id}
                  icon={section.icon}
                  title={t(section.labelKey)}
                  links={section.links}
                />
              ))}
              <NavLink to="/settings" icon={Settings}>
                {t("common.settings")}
              </NavLink>
            </div>
          </div>
        )}
      </div>
    </aside>
  );
}

function SidebarSection({
  icon: Icon,
  title,
  action,
  children,
}: {
  icon: LucideIcon;
  title: string;
  action?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <div className="space-y-0.5">
      <div className="flex h-8 items-center justify-between gap-1 px-1.5">
        <div className={cn(SIDEBAR_SECTION_TITLE_CLASS, "min-w-0 flex-1 gap-2")}>
          <Icon className="h-4 w-4 shrink-0 opacity-80" />
          <span className="min-w-0 truncate">{title}</span>
        </div>
        {action ? <div className="flex h-7 w-7 shrink-0 items-center justify-center">{action}</div> : null}
      </div>
      <div className="pl-0.5">{children}</div>
    </div>
  );
}

function CollapsibleLinkGroup({
  icon: Icon,
  title,
  links,
}: {
  icon: LucideIcon;
  title: string;
  links: { to: string; labelKey: string; icon: LucideIcon }[];
}) {
  const { t } = useTranslation();
  const pendingCandidates = usePendingCandidateCount();

  return (
    <details className="group">
      <summary
        className={cn(
          SIDEBAR_SECTION_TITLE_CLASS,
          "hover:bg-sidebar-accent/60 hover:text-foreground cursor-pointer list-none justify-between rounded-md px-1.5 [&::-webkit-details-marker]:hidden",
        )}
      >
        <span className="flex min-w-0 items-center gap-2">
          <Icon className="h-4 w-4 shrink-0 opacity-80" />
          <span className="min-w-0 truncate">{title}</span>
        </span>
        <ChevronRight className="text-muted-foreground h-3.5 w-3.5 shrink-0 transition-transform group-open:rotate-90" />
      </summary>
      <ul className="mt-0.5 space-y-0.5 pb-1 pl-0.5">
        {links.map((link) => (
          <li key={link.to}>
            <NavLink
              to={link.to}
              icon={link.icon}
              badge={link.to === "/memory" && pendingCandidates > 0 ? pendingCandidates : undefined}
            >
              {t(link.labelKey)}
            </NavLink>
          </li>
        ))}
      </ul>
    </details>
  );
}

function NavLink({
  to,
  icon: Icon,
  children,
  badge,
}: {
  to: string;
  icon: LucideIcon;
  children: React.ReactNode;
  badge?: number;
}) {
  return (
    <Link
      to={to}
      className={SIDEBAR_LEAF_CLASS}
      activeProps={{
        className: SIDEBAR_LEAF_ACTIVE_CLASS,
      }}
    >
      <Icon className="h-3.5 w-3.5 shrink-0" />
      <span className="min-w-0 flex-1 truncate">{children}</span>
      {badge != null && badge > 0 ? (
        <span className="bg-primary text-primary-foreground rounded-full px-1.5 py-0.5 text-[10px] font-medium tabular-nums">
          {badge > 99 ? "99+" : badge}
        </span>
      ) : null}
    </Link>
  );
}

function IconNavLink({ to, icon: Icon, label }: { to: string; icon: LucideIcon; label: string }) {
  return (
    <Link
      to={to}
      title={label}
      aria-label={label}
      className="text-muted-foreground hover:bg-sidebar-accent hover:text-foreground flex h-8 w-8 items-center justify-center rounded-md transition-colors"
      activeProps={{
        className:
          "flex h-8 w-8 items-center justify-center rounded-md bg-sidebar-accent text-foreground",
      }}
    >
      <Icon className="h-4 w-4" />
    </Link>
  );
}
