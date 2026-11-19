import type { ComponentType, ReactNode, RefObject } from "react";
export const Modal: ComponentType<{isOpen: boolean; onClose: () => void; title: string; children: ReactNode; footer?: ReactNode; size?: string; draggable?: boolean; storageKey?: string; className?: string; closeOnOverlayClick?: boolean; closeOnEscape?: boolean; initialFocusRef?: RefObject<HTMLElement | null>}>;
