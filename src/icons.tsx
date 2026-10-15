/**
 * Icon system — the single place the app picks an icon glyph, its size and its
 * weight.
 *
 * Convention (applies everywhere; do not import from `@phosphor-icons/react`
 * in a component):
 *
 *  - **Glyph**  Components use a *semantic* name (`transfer`, `warning`,
 *    `bellEmpty`) rather than a picture name, so the glyph behind a meaning can
 *    change in one place. The registry below is the only mapping.
 *  - **Size**   Only the steps in `ICON_SIZE` are used. The previous code had
 *    eleven ad-hoc sizes (8, 14, 15, 16, 17, 18, 20, 26, 27, 28 …) which made
 *    the interface subtly uneven; the scale is now the whole vocabulary.
 *  - **Weight** `regular` is the default. Small sizes step up to `bold` because
 *    Phosphor strokes thin out below ~14 px, and severity marks use `fill` so a
 *    status glyph reads as a solid mark rather than an outline. No other
 *    weights are used — `duotone`/`thin` would fight the restrained, flat
 *    Mac-inspired surface.
 *  - **Color**  Icons never hard-code a colour: they inherit `currentColor`, so
 *    the existing light/dark design tokens drive them. An explicit `color` is
 *    accepted only for exceptional cases.
 *  - **Spacing** Icons carry no margin. Gap and padding come from the parent's
 *    flex layout, so spacing stays a layout concern.
 *  - **Hover/focus** Interactive icons live inside a real `<button>`; the
 *    button's `:hover`/`:focus-visible`/`:disabled` rules animate the colour
 *    they inherit. Never attach interaction to the `<svg>` itself.
 *
 * Accessibility is structural, not advisory:
 *  - An icon with no `label` is decorative: it is emitted with `aria-hidden`
 *    and `focusable="false"`, so screen readers skip it.
 *  - An icon with a `label` becomes `role="img"` with that `aria-label`.
 *  - `IconButton` **requires** a `label` in its type, so an icon-only control
 *    cannot be added without an accessible name.
 */

import type { Icon as PhosphorIcon, IconWeight } from "@phosphor-icons/react";
import {
  ArrowsClockwiseIcon,
  ArrowRightIcon,
  BellIcon,
  BellRingingIcon,
  BellSlashIcon,
  CheckCircleIcon,
  ChecksIcon,
  CheckIcon,
  CircleNotchIcon,
  ClockCounterClockwiseIcon,
  DeviceMobileIcon,
  DownloadSimpleIcon,
  FlaskIcon,
  FolderSimpleIcon,
  FunnelIcon,
  ImageIcon,
  InfoIcon,
  MinusIcon,
  PlayIcon,
  QuestionIcon,
  ShieldWarningIcon,
  StopCircleIcon,
  TrashIcon,
  UsbIcon,
  VideoCameraIcon,
  WarningIcon,
  XCircleIcon,
  XIcon,
} from "@phosphor-icons/react";

/** The complete size vocabulary. Use these steps, never a raw number. */
export const ICON_SIZE = {
  /** Tiny marks inside a badge or thumbnail overlay. */
  badge: 10,
  /** Inline text-level marks and table affordances. */
  micro: 12,
  /** Compact controls and list rows. */
  small: 14,
  /** Default body-level icon. */
  medium: 16,
  /** Sidebar navigation rows. */
  nav: 17,
  /** Primary action buttons. */
  action: 18,
  /** Panel and banner headers. */
  large: 20,
  /** Empty-state artwork. */
  empty: 27,
  /** Setup-screen focal icon. */
  hero: 30,
} as const;

export type IconSize = keyof typeof ICON_SIZE;

/** Semantic name → Phosphor glyph. Add new meanings here, not in components. */
const GLYPHS = {
  // Identity and structure
  brand: DownloadSimpleIcon,
  device: DeviceMobileIcon,
  allMedia: FolderSimpleIcon,
  photos: ImageIcon,
  videos: VideoCameraIcon,
  help: QuestionIcon,

  // Connection
  usb: UsbIcon,
  refresh: ArrowsClockwiseIcon,
  spinner: CircleNotchIcon,

  // Status and severity
  success: CheckCircleIcon,
  warning: WarningIcon,
  error: XCircleIcon,
  info: InfoIcon,
  security: ShieldWarningIcon,
  stopped: StopCircleIcon,

  // Files and transfer
  play: PlayIcon,
  transfer: DownloadSimpleIcon,
  destination: FolderSimpleIcon,
  /** A file the transfer deliberately left alone (already present). */
  skipped: MinusIcon,
  /** Demo mode: exercises the UI against a clearly labelled mock device. */
  demo: FlaskIcon,

  // Actions
  close: XIcon,
  /** Cancel a running operation. */
  cancel: XCircleIcon,
  check: CheckIcon,
  markAllRead: ChecksIcon,
  dismiss: TrashIcon,
  filter: FunnelIcon,
  goTo: ArrowRightIcon,
  pass: CheckIcon,
  fail: XIcon,
  neutral: MinusIcon,

  // Notifications
  bell: BellIcon,
  bellActive: BellRingingIcon,
  bellEmpty: BellSlashIcon,
  history: ClockCounterClockwiseIcon,
} as const satisfies Record<string, PhosphorIcon>;

export type IconName = keyof typeof GLYPHS;

/**
 * Weights that deviate from `regular`, by size. Small glyphs need more
 * presence; severity marks read better solid.
 */
const WEIGHT_BY_SIZE: Partial<Record<IconSize, IconWeight>> = {
  badge: "bold",
  micro: "bold",
};

interface IconProps {
  name: IconName;
  /** Step from `ICON_SIZE`. Defaults to `medium`. */
  size?: IconSize;
  /** Overrides the size-derived default. Use sparingly. */
  weight?: IconWeight;
  /** Merged onto the `<svg>`; use for layout/rotation utilities such as `spin`. */
  className?: string;
  /** Only for exceptional cases — icons otherwise inherit `currentColor`. */
  color?: string;
  /**
   * Accessible name. Provide it when the icon is the *only* carrier of meaning.
   * Omit it when adjacent text already says the same thing (the icon is then
   * hidden from assistive technology).
   */
  label?: string;
}

/**
 * One icon glyph, sized and weighted by the convention above.
 *
 * @example
 * // Decorative: the button already has visible text.
 * <Icon name="transfer" size="action" />
 * // Meaningful: a status glyph with no adjacent words.
 * <Icon name="warning" size="medium" label="Warning" />
 */
export function Icon({ name, size = "medium", weight, className, color, label }: IconProps) {
  const Glyph = GLYPHS[name];
  const resolved: IconWeight = weight ?? WEIGHT_BY_SIZE[size] ?? "regular";

  if (label === undefined) {
    return (
      <Glyph
        size={ICON_SIZE[size]}
        weight={resolved}
        className={className}
        color={color}
        aria-hidden="true"
        focusable="false"
      />
    );
  }

  return (
    <Glyph
      size={ICON_SIZE[size]}
      weight={resolved}
      className={className}
      color={color}
      role="img"
      aria-label={label}
    />
  );
}

type IconButtonProps = {
  icon: IconName;
  /**
   * Required accessible name. An icon-only control has no visible text, so
   * without this a screen reader would announce an unnamed "button".
   */
  label: string;
  size?: IconSize;
  weight?: IconWeight;
  className?: string;
  onClick: (event: React.MouseEvent<HTMLButtonElement>) => void;
  disabled?: boolean;
  /** Count rendered as a small badge; announced through `label`, not twice. */
  badge?: number;
  /** Set for disclosures such as the notification panel. */
  expanded?: boolean;
  /** id of the element this control shows/hides. */
  controls?: string;
  hasPopup?: "dialog" | "menu" | "listbox";
  /** Tooltip text; defaults to the accessible name. */
  title?: string;
  type?: "button" | "submit";
  /** `aria-pressed` for toggle controls. */
  pressed?: boolean;
  /** Forwarded to the `<button>` so callers can restore focus after closing. */
  buttonRef?: React.Ref<HTMLButtonElement>;
};

/**
 * An icon-only `<button>` that cannot be written without an accessible name.
 * Communicating the state that matters (expanded, pressed, count) is part of
 * the same contract, so callers describe the control rather than the picture.
 */
export function IconButton({
  icon,
  label,
  size = "medium",
  weight,
  className,
  onClick,
  disabled,
  badge,
  expanded,
  controls,
  hasPopup,
  title,
  type = "button",
  pressed,
  buttonRef,
}: IconButtonProps) {
  return (
    <button
      ref={buttonRef}
      type={type}
      className={className}
      onClick={onClick}
      disabled={disabled}
      title={title ?? label}
      aria-label={label}
      aria-expanded={expanded}
      aria-controls={controls}
      aria-haspopup={hasPopup}
      aria-pressed={pressed}
    >
      <Icon name={icon} size={size} weight={weight} />
      {badge !== undefined && badge > 0 ? (
        <span className="icon-badge" aria-hidden="true">
          {badge > 99 ? "99+" : badge}
        </span>
      ) : null}
    </button>
  );
}

/** True when a semantic name is available; used by tests and dev assertions. */
export function isIconName(name: string): name is IconName {
  return Object.prototype.hasOwnProperty.call(GLYPHS, name);
}

/** All registered semantic names — handy for tests and documentation. */
export const ICON_NAMES = Object.keys(GLYPHS) as IconName[];
