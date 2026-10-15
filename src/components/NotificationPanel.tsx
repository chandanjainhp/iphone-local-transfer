import { useEffect, useMemo, useRef, useState } from "react";
import { formatRelativeTime } from "../format";
import { Icon, IconButton } from "../icons";
import {
  CATEGORY_LABELS,
  filterNotifications,
  presentCategories,
  unreadCount,
  type AppNotification,
  type NotificationCategory,
  type NotificationCategoryFilter,
  type NotificationFilter,
  type NotificationSeverity,
} from "../notifications";

interface NotificationPanelProps {
  id: string;
  notifications: AppNotification[];
  filter: NotificationFilter;
  onFilter: (filter: NotificationFilter) => void;
  /** Narrow the list to one category, or `"any"` for no scoping. */
  category: NotificationCategoryFilter;
  onCategory: (category: NotificationCategoryFilter) => void;
  onMarkRead: (id: string) => void;
  onMarkUnread: (id: string) => void;
  onMarkAllRead: () => void;
  onDismiss: (id: string) => void;
  onClearAll: () => void;
  /** Click-through: read the row and navigate to the related part of the app. */
  onActivate: (notification: AppNotification) => void;
  onClose: () => void;
}

/** Severity → glyph. Severity is also stated in text, so the glyph is decorative. */
const SEVERITY_GLYPH: Record<NotificationSeverity, "success" | "warning" | "error" | "info"> = {
  success: "success",
  warning: "warning",
  error: "error",
  info: "info",
};

/** Re-render on a slow tick so "4 min" does not go stale while the panel is open. */
function useNow(intervalMs: number): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), intervalMs);
    return () => window.clearInterval(timer);
  }, [intervalMs]);
  return now;
}

/**
 * Dropdown notification centre.
 *
 * Deliberately a non-modal popover (`aria-modal` is not set and focus is not
 * trapped): it behaves like a macOS menu-bar panel, where the rest of the
 * window stays reachable. Escape closes it, clicking outside closes it, and
 * focus is moved into the panel on open and returned to the bell on close —
 * all handled here or by the caller.
 */
export function NotificationPanel({
  id,
  notifications,
  filter,
  onFilter,
  category,
  onCategory,
  onMarkRead,
  onMarkUnread,
  onMarkAllRead,
  onDismiss,
  onClearAll,
  onActivate,
  onClose,
}: NotificationPanelProps) {
  const panelRef = useRef<HTMLDivElement>(null);
  const now = useNow(30_000);

  // Counts describe what the controls currently yield, so the chips can never
  // advertise rows the active category filter is hiding.
  const scoped = useMemo(
    () => (category === "any" ? notifications : notifications.filter((n) => n.category === category)),
    [notifications, category],
  );
  // The active category stays in the list even after its last row is
  // dismissed. Otherwise the control would be filtered by a value it no longer
  // offers, and a native `<select>` whose value is outside its options renders
  // blank — leaving an empty panel with no visible reason why.
  const categories = useMemo(() => {
    const present = presentCategories(notifications);
    return category !== "any" && !present.includes(category) ? [...present, category] : present;
  }, [notifications, category]);
  const visible = useMemo(() => filterNotifications(notifications, filter, category), [
    notifications,
    filter,
    category,
  ]);
  const unread = unreadCount(scoped);
  const total = scoped.length;
  /** The category filter is hiding something, so the empty state must say so. */
  const filtered = category !== "any";
  const categoryLabel = filtered ? CATEGORY_LABELS[category as NotificationCategory] : "";

  /** Copy for the empty state, which has three distinct causes. */
  const empty = (() => {
    if (notifications.length === 0) {
      return {
        heading: "You're all caught up",
        body: "Device, scan and transfer activity will appear here.",
        offerAllCategories: false,
      };
    }
    if (filtered && total === 0) {
      return {
        heading: `No ${categoryLabel.toLowerCase()} activity`,
        body: "This category has no notifications. Show every category to see the rest.",
        offerAllCategories: true,
      };
    }
    if (filtered) {
      return {
        heading: "Nothing unread in this category",
        body: `Switch to All to see the ${categoryLabel.toLowerCase()} notifications you have already read.`,
        offerAllCategories: false,
      };
    }
    return {
      heading: "Nothing unread",
      body: "Switch to All to see the notifications you have already read.",
      offerAllCategories: false,
    };
  })();

  // Escape closes the panel. Bound on the document so it works while focus is
  // anywhere inside the popover.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.stopPropagation();
        onClose();
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onClose]);

  // Move focus into the panel so keyboard users land in the newly opened
  // surface instead of staying on the bell.
  useEffect(() => {
    panelRef.current?.focus();
  }, []);

  return (
    <div
      id={id}
      ref={panelRef}
      className="notif-panel"
      role="dialog"
      aria-label="Notifications"
      tabIndex={-1}
    >
      <div className="notif-head">
        <h2>
          Notifications
          {unread > 0 ? <span className="notif-head-count">{unread} new</span> : null}
        </h2>
        <div className="notif-head-actions">
          <button
            type="button"
            className="ghost-button notif-mark-all"
            onClick={onMarkAllRead}
            disabled={unread === 0}
          >
            <Icon name="markAllRead" size="small" />
            Mark all as read
          </button>
          <IconButton
            icon="close"
            label="Close notifications"
            size="small"
            className="icon-button"
            onClick={onClose}
          />
        </div>
      </div>

      <div className="notif-filters" role="group" aria-label="Filter notifications">
        <button
          type="button"
          className={`notif-filter ${filter === "all" ? "active" : ""}`}
          aria-pressed={filter === "all"}
          onClick={() => onFilter("all")}
        >
          All
          <span className="notif-filter-count">{total}</span>
        </button>
        <button
          type="button"
          className={`notif-filter ${filter === "unread" ? "active" : ""}`}
          aria-pressed={filter === "unread"}
          onClick={() => onFilter("unread")}
        >
          Unread
          <span className="notif-filter-count">{unread}</span>
        </button>

        {/* Only offered when there is more than one category to choose
            between: a picker with a single option is noise. A native select
            keeps keyboard and screen-reader behaviour for free and matches the
            conflict-policy control already used in the toolbar. */}
        {categories.length > 1 ? (
          <label className="notif-category">
            <span className="sr-only">Filter by category</span>
            <select
              className="notif-category-select"
              value={category}
              onChange={(event) =>
                onCategory(event.target.value as NotificationCategoryFilter)
              }
            >
              <option value="any">All categories</option>
              {categories.map((value) => (
                <option key={value} value={value}>
                  {CATEGORY_LABELS[value]}
                </option>
              ))}
            </select>
          </label>
        ) : null}
      </div>

      {visible.length === 0 ? (
        <div className="notif-empty">
          <Icon name="bellEmpty" size="empty" />
          <h3>{empty.heading}</h3>
          <p>{empty.body}</p>
          {empty.offerAllCategories ? (
            <button
              type="button"
              className="ghost-button notif-empty-reset"
              onClick={() => onCategory("any")}
            >
              <Icon name="filter" size="small" />
              Show all categories
            </button>
          ) : null}
          {notifications.length === 0 ? (
            <span className="notif-empty-note">
              Notifications are stored on this computer only.
            </span>
          ) : null}
        </div>
      ) : (
        <ul className="notif-list">
          {visible.map((notification) => {
            const relative = formatRelativeTime(notification.createdAt, now);
            const isProblem = notification.severity === "error" || notification.severity === "warning";
            return (
              <li
                key={notification.id}
                className={`notif-item ${notification.severity} ${notification.read ? "read" : "unread"}`}
              >
                <span className="notif-severity">
                  {/* Decorative: the title text carries the meaning, and the
                      severity is also stated in words for screen readers. */}
                  <Icon name={SEVERITY_GLYPH[notification.severity]} size="medium" />
                </span>

                <div className="notif-main">
                  <button
                    type="button"
                    className="notif-activate"
                    onClick={() => onActivate(notification)}
                    disabled={notification.target === null}
                    title={
                      notification.target === null
                        ? undefined
                        : "Go to the related part of the app"
                    }
                  >
                    <span className="notif-title">
                      {notification.title}
                      {/* Unread is signalled visually by the row background;
                          screen readers need it stated. */}
                      {!notification.read ? <span className="sr-only">Unread.</span> : null}
                      {notification.count > 1 ? (
                        <span className="notif-repeat" title={`Seen ${notification.count} times`}>
                          ×{notification.count}
                        </span>
                      ) : null}
                    </span>
                    {notification.body ? (
                      <span className="notif-body">{notification.body}</span>
                    ) : null}
                    <span className="notif-meta">
                      <span className="notif-severity-word">
                        {notification.severity === "error"
                          ? "Error"
                          : notification.severity === "warning"
                            ? "Warning"
                            : notification.severity === "success"
                              ? "Success"
                              : "Info"}
                      </span>
                      <span className="notif-dot-sep" aria-hidden="true">
                        ·
                      </span>
                      <time dateTime={new Date(notification.createdAt).toISOString()}>
                        {relative}
                      </time>
                      {notification.target !== null ? (
                        <>
                          <span className="notif-dot-sep" aria-hidden="true">
                            ·
                          </span>
                          <span className="notif-goto">
                            Open
                            <Icon name="goTo" size="micro" />
                          </span>
                        </>
                      ) : null}
                    </span>
                  </button>
                </div>

                {/* Sibling of the content column, not a child of it: the
                    controls belong at the row's right edge, level with the
                    title, rather than flowing underneath the text. */}
                <div className="notif-item-actions">
                  <IconButton
                    icon={notification.read ? "bell" : "check"}
                    label={
                      notification.read
                        ? `Mark "${notification.title}" as unread`
                        : `Mark "${notification.title}" as read`
                    }
                    size="small"
                    className="icon-button"
                    onClick={() =>
                      notification.read ? onMarkUnread(notification.id) : onMarkRead(notification.id)
                    }
                  />
                  <IconButton
                    icon="dismiss"
                    label={`Dismiss "${notification.title}"`}
                    size="small"
                    className="icon-button"
                    onClick={() => onDismiss(notification.id)}
                  />
                  {isProblem && !notification.read ? (
                    <span className="notif-unread-dot" aria-hidden="true" />
                  ) : null}
                </div>
              </li>
            );
          })}
        </ul>
      )}

      {total > 0 ? (
        <div className="notif-foot">
          <button type="button" className="ghost-button" onClick={onClearAll}>
            <Icon name="dismiss" size="small" />
            Clear all
          </button>
          <span className="notif-foot-note">Stored on this computer only</span>
        </div>
      ) : null}
    </div>
  );
}
