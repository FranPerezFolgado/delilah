# UI contract: `components/ui/Notice.tsx`

The only interface this feature adds; internal to the web UI (no HTTP or IPC change).

```ts
/** The one close control for notifications: IconButton, icon "close", accessible name t("Cerrar"). */
export function CloseButton(props: { onClick: () => void; className?: string }): JSX.Element;

/** A bar/banner: the caller's content and actions, then the CloseButton, last. */
export function Notice(props: {
  className?: string;          // the caller's own classes (gn-bar, ts-bar, dcb…), kept as is
  role?: "status" | "alert";
  onClose: () => void;
  children: React.ReactNode;
}): JSX.Element;

/** Per-id dismissals that last while each id's signature is unchanged. */
export function useDismissals(current: Record<string, string>): {
  hidden: (id: string) => boolean;
  dismiss: (id: string) => void;
};

/** One notice; `null` = not showing (forgets the dismissal). */
export function useDismissal(sig: string | null): [hidden: boolean, dismiss: () => void];
```

Guarantees:

- `Notice` renders exactly one button named "Cerrar" of its own, after `children`.
- Clicking it calls `onClose` once; if it had focus, focus moves to the next focusable element after the notice.
- `CloseButton` has class `ntc-x` (tests and SC-005 can check every notification uses it).
- `useDismissals` never hides an id whose signature changed since `dismiss`.
