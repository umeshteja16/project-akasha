import type { ReactNode } from "react";

/** Two-column settings row: explanation on the left, controls on the right. */
export function SettingsSection({
  id,
  title,
  description,
  children,
}: {
  id: string;
  title: string;
  description: ReactNode;
  children: ReactNode;
}) {
  return (
    <section
      aria-labelledby={`${id}-title`}
      className="grid gap-4 border-b border-border py-8 last:border-b-0 md:grid-cols-[minmax(0,1fr)_minmax(0,2fr)] md:gap-10"
    >
      <div className="grid content-start gap-1">
        <h2 id={`${id}-title`} className="display text-xl text-fg">
          {title}
        </h2>
        <p className="text-sm text-fg-muted">{description}</p>
      </div>
      <div>{children}</div>
    </section>
  );
}
