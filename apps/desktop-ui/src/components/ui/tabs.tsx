import * as React from "react";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "@/lib/utils";

const tabsTriggerVariants = cva("rounded-md transition-colors", {
  variants: {
    variant: {
      default: "px-3 py-1.5 text-sm font-medium",
      compact: "flex h-8 min-w-14 shrink-0 items-center justify-center gap-1 px-2 text-[11px]",
    },
    active: {
      true: "bg-muted text-foreground",
      false: "text-muted-foreground",
    },
  },
  compoundVariants: [
    { variant: "default", active: false, class: "hover:bg-muted/50 hover:text-foreground" },
    { variant: "compact", active: true, class: "shadow-xs" },
    { variant: "compact", active: false, class: "hover:bg-muted hover:text-foreground" },
  ],
  defaultVariants: {
    variant: "default",
    active: false,
  },
});

const TabsList = React.forwardRef<HTMLDivElement, React.HTMLAttributes<HTMLDivElement>>(
  ({ className, ...props }, ref) => (
    <div ref={ref} role="tablist" className={cn(className)} {...props} />
  ),
);
TabsList.displayName = "TabsList";

export interface TabsTriggerProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement>, VariantProps<typeof tabsTriggerVariants> {}

const TabsTrigger = React.forwardRef<HTMLButtonElement, TabsTriggerProps>(
  ({ className, variant, active, type = "button", ...props }, ref) => (
    <button
      ref={ref}
      type={type}
      role="tab"
      aria-selected={active ?? false}
      className={cn(tabsTriggerVariants({ variant, active, className }))}
      {...props}
    />
  ),
);
TabsTrigger.displayName = "TabsTrigger";

const TabsContent = React.forwardRef<HTMLDivElement, React.HTMLAttributes<HTMLDivElement>>(
  ({ className, ...props }, ref) => (
    <div ref={ref} role="tabpanel" tabIndex={0} className={cn(className)} {...props} />
  ),
);
TabsContent.displayName = "TabsContent";

export { TabsList, TabsTrigger, TabsContent };
