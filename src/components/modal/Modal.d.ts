import type { ComponentType, ReactNode } from "react";
export const Modal: ComponentType<{isOpen: boolean; onClose: () => void; title: string; children: ReactNode; footer?: ReactNode; size?: string; draggable?: boolean; storageKey?: string; className?: string}>;
