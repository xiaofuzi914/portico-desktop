import type { ComponentProps } from "react";
import { cn } from "@/lib/utils";

type PorticoMarkProps = Omit<ComponentProps<"img">, "src" | "alt" | "aria-hidden" | "draggable">;

export function PorticoMark({ className, ...props }: PorticoMarkProps) {
  return (
    <img
      {...props}
      src="/portico-mark.svg"
      alt=""
      aria-hidden="true"
      draggable={false}
      className={cn("block shrink-0 select-none", className)}
    />
  );
}
