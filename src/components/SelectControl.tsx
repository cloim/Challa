import { ChevronDown } from "lucide-react";
import type { SelectHTMLAttributes } from "react";
export function SelectControl({ children, className = "", ...props }: SelectHTMLAttributes<HTMLSelectElement>) {
  return <span className="relative flex w-full items-center"><select {...props} className={`field w-full appearance-none pr-9 ${className}`}>{children}</select><ChevronDown size={14} className="pointer-events-none absolute right-3 text-muted" aria-hidden="true" /></span>;
}
