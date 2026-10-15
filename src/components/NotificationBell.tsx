import { IconButton } from "../icons";

interface NotificationBellProps {
  /** Real unread count from the store — never a hardcoded value. */
  unread: number;
  open: boolean;
  onToggle: () => void;
  /** id of the panel this button discloses. */
  panelId: string;
  /** Lets the panel return focus here when it closes. */
  buttonRef?: React.Ref<HTMLButtonElement>;
}

/**
 * Bell that opens the notification panel.
 *
 * The unread count is announced through the button's accessible name (so a
 * screen reader hears "Notifications, 3 unread") and the visible badge is
 * `aria-hidden`, so the number is never read twice.
 */
export function NotificationBell({ unread, open, onToggle, panelId, buttonRef }: NotificationBellProps) {
  const label =
    unread === 0
      ? "Notifications, none unread"
      : `Notifications, ${unread} unread`;

  return (
    <IconButton
      icon={unread > 0 ? "bellActive" : "bell"}
      label={label}
      size="large"
      // `icon-button` comes first: it carries the whole control grammar —
      // padding, cursor, radius, hover, press, disabled. Without it this button
      // fell back to the browser's default chrome (outset border, default
      // cursor) instead of the app's.
      className={`icon-button notif-bell${open ? " open" : ""}`}
      onClick={onToggle}
      expanded={open}
      controls={panelId}
      hasPopup="dialog"
      badge={unread}
      buttonRef={buttonRef}
    />
  );
}
