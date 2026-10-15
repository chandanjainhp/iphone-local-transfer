import { useEffect, useRef, type RefObject } from "react";

/**
 * Close a popover when the pointer goes down outside `containerRef`.
 *
 * `pointerdown` rather than `click` so the popover closes on press, matching
 * native menus, and so a drag that starts inside and ends outside does not
 * close it. The handler is only attached while `active`, so a closed popover
 * costs nothing and cannot swallow unrelated clicks.
 */
export function useCloseOnOutsidePointerDown(
  active: boolean,
  containerRef: RefObject<HTMLElement | null>,
  onClose: () => void,
): void {
  useEffect(() => {
    if (!active) return;
    const onPointerDown = (event: PointerEvent) => {
      const node = containerRef.current;
      const target = event.target;
      if (!node || !(target instanceof Node)) return;
      if (!node.contains(target)) onClose();
    };
    document.addEventListener("pointerdown", onPointerDown);
    return () => document.removeEventListener("pointerdown", onPointerDown);
  }, [active, containerRef, onClose]);
}

/**
 * Keep a ref pointing at the latest value without re-subscribing effects.
 * Used for event callbacks that must not re-register Tauri listeners.
 */
export function useLatest<T>(value: T): RefObject<T> {
  const ref = useRef(value);
  ref.current = value;
  return ref;
}
