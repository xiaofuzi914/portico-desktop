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
import { buildNavigationSections } from "./navigation-model";
import {
  COLLAPSED_SIDEBAR_WIDTH,
  DEFAULT_SIDEBAR_WIDTH,
  readSidebarCollapsed,
  writeSidebarCollapsed,
} from "./shell-layout-state";

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
            className="bg-primary text-primary-foreground focus-visible:ring-ring flex h-7 w-7 items-center justify-center rounded-md text-xs font-semibold focus-visible:ring-2 focus-visible:outline-none"
            onClick={() => setSidebarCollapsed(false)}
            aria-label={t("sidebar.expand")}
            title={t("sidebar.expand")}
          >
            P
          </button>
        ) : (
          <>
            <div className="bg-primary text-primary-foreground flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-xs font-semibold">
              P
            </div>
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
          "flex flex-1 flex-col overflow-y-auto py-3",
          collapsed ? "items-center px-1.5" : "px-3",
        )}
      >
        {collapsed ? (
          <>
            <div className="flex w-full flex-col items-center gap-1">
              <SidebarProjectActions compact />
              <SidebarProjects activeWorkspaceId={workspaceId} compact />
            </div>
            <div className="mt-auto flex flex-col items-center gap-1 pt-4">
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
          <>
            <div className="space-y-5">
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

            <div className="mt-auto space-y-2 pt-5">
              <div className="border-t pt-3">
                {navigationSections.map((section) => (
                  <CollapsibleLinkGroup
                    key={section.id}
                    title={t(section.labelKey)}
                    links={section.links}
                  />
                ))}
              </div>
              <NavLink to="/settings" icon={Settings}>
                {t("common.settings")}
              </NavLink>
            </div>
          </>
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
    <div className="space-y-2">
      <div className="flex h-7 items-center justify-between gap-2 px-2">
        <div className="text-muted-foreground flex min-w-0 items-center gap-2 text-[11px] font-semibold tracking-wide uppercase">
          <Icon className="h-3.5 w-3.5 shrink-0" />
          <span className="min-w-0 truncate">{title}</span>
        </div>
        {action}
      </div>
      {children}
    </div>
  );
}

function CollapsibleLinkGroup({
  title,
  links,
}: {
  title: string;
  links: { to: string; labelKey: string; icon: LucideIcon }[];
}) {
  const { t } = useTranslation();

  return (
    <details className="group">
      <summary className="text-muted-foreground hover:text-foreground flex h-8 cursor-pointer list-none items-center justify-between rounded-md px-2 text-[11px] font-semibold tracking-wide uppercase">
        <span>{title}</span>
        <ChevronRight className="h-3.5 w-3.5 transition-transform group-open:rotate-90" />
      </summary>
      <ul className="mt-1 space-y-0.5 pb-2">
        {links.map((link) => (
          <li key={link.to}>
            <NavLink to={link.to} icon={link.icon}>
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
}: {
  to: string;
  icon: LucideIcon;
  children: React.ReactNode;
}) {
  return (
    <Link
      to={to}
      className="text-muted-foreground hover:bg-sidebar-accent hover:text-foreground group flex h-8 items-center gap-2 rounded-md px-2 text-sm transition-colors"
      activeProps={{
        className:
          "flex h-8 items-center gap-2 rounded-md px-2 text-sm bg-sidebar-accent font-medium text-foreground",
      }}
    >
      <Icon className="h-4 w-4 shrink-0" />
      <span className="min-w-0 flex-1 truncate">{children}</span>
      <ChevronRight className="h-3.5 w-3.5 opacity-0 transition-opacity group-hover:opacity-60" />
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
