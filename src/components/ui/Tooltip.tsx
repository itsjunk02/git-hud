import type { ReactNode } from "react";
import * as RadixTooltip from "@radix-ui/react-tooltip";

/** Minimal Radix tooltip wrapper used across the HUD. */
export function Tooltip({
  label,
  side = "right",
  children,
}: {
  label: string;
  side?: "top" | "right" | "bottom" | "left";
  children: ReactNode;
}) {
  return (
    <RadixTooltip.Root>
      <RadixTooltip.Trigger asChild>{children}</RadixTooltip.Trigger>
      <RadixTooltip.Portal>
        <RadixTooltip.Content
          side={side}
          sideOffset={8}
          className="z-50 rounded-md bg-zinc-800 px-2 py-1 text-xs text-zinc-100 shadow-lg ring-1 ring-white/10 select-none"
        >
          {label}
          <RadixTooltip.Arrow className="fill-zinc-800" />
        </RadixTooltip.Content>
      </RadixTooltip.Portal>
    </RadixTooltip.Root>
  );
}

export { Provider as TooltipProvider } from "@radix-ui/react-tooltip";
