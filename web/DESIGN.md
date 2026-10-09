# Akasha design system

Akasha is a private second memory: a place to keep documents, scans, images and notes,
find them again, and ask questions answered from them. The interface should feel like a
well-made notebook or a quiet reading room, not an admin dashboard.

**Direction: calm, focused, editorial.** Information-dense where it matters (lists,
results, citations), airy everywhere else. Typography carries the identity; colour is
restrained, with one accent used sparingly.

Implementation: tokens in [`src/styles/tokens.css`](src/styles/tokens.css), mapped onto
Tailwind v4 in [`src/styles/app.css`](src/styles/app.css). Primitives live in
`src/components/ui/` (shadcn/ui-style, owned code on Radix). A living reference renders
at `/design` (command palette → "Design system").

## Principles

1. **Paper and ink.** Warm off-white paper in light mode, warm near-black ink in dark
   mode. Never pure `#fff` page backgrounds or pure `#000`.
2. **Type first.** Hierarchy comes from the serif display face, size and spacing before
   it comes from colour, boxes or icons.
3. **One accent.** Verdigris (a deep teal-green) marks the primary action, the active
   place and focus. If two things on screen are accent-coloured, one of them is wrong.
   The saffron *highlight* appears only as the wordmark dot and search `<mark>`s.
4. **Hairlines, not boxes.** Separate with 1px rules and whitespace; cards only where
   content is a unit (a settings group, a file).
5. **Quiet motion.** Short, eased fades and rises that confirm a change; nothing loops
   except loading skeletons, and nothing moves under `prefers-reduced-motion`.
6. **Honest empty states.** Say what will appear and how to get there, in plain words.

## Typography

Self-hosted with `@fontsource-variable/*` (no CDN; the app works offline).

| Role | Family | Use |
|---|---|---|
| Display | **Newsreader** (variable, optical sizes) | Page titles, section titles, dialog titles, the wordmark (italic), empty-state titles, pull quotes |
| UI | **Inter** (variable, optical sizes) | Everything interactive and most text |
| Mono | **JetBrains Mono** (variable) | Excerpts, page/offset references, keyboard keys, eyebrow labels |

Scale (`text-*`, rem at 16px root):

| Token | Size / line | Typical use |
|---|---|---|
| `2xs` | 11 / 16 | eyebrow labels, badges, tab-bar labels |
| `xs` | 12 / 18 | hints, metadata |
| `sm` | 14 / 22 | **default UI text** (body default) |
| `base` | 16 / 26 | reading text: answers, extracted text, descriptions on narrow screens |
| `lg` | 18 / 28 | lead paragraphs |
| `xl` | 22 / 30 | section titles (display) |
| `2xl` | 28 / 36 | empty-state and dialog-level titles (display) |
| `3xl` | 36 / 42 | page titles on phones, auth titles (display) |
| `4xl` | 48 / 52 | page titles on desktop, the auth pull quote (display) |

Utilities: `display` (serif, optical sizing, weight 420, slight negative tracking),
`eyebrow` (mono, 11px, uppercase, 0.08em tracking, subtle colour). Reading measure is
`--reading-max: 68ch`. Headings use `text-wrap: balance`, paragraphs `pretty`.

## Colour

All colours are CSS custom properties that switch with the theme; components only use
the semantic names (`bg-surface`, `text-fg-muted`, `border-border`, ...), never hex.

| Token | Light | Dark | Role |
|---|---|---|---|
| `bg` | `#f7f5f0` | `#141412` | page |
| `surface` | `#ffffff` | `#1b1a18` | cards, inputs, menus |
| `surface-2` / `-3` | `#efece5` / `#e7e3da` | `#25241f` / `#2e2c27` | hover, sunken, skeletons |
| `sidebar` | `#f1eee7` | `#171614` | navigation rail, auth aside |
| `border` | `#e3dfd6` | `#2e2c27` | hairlines (decorative) |
| `border-strong` | `#938c7f` | `#6f6a60` | control outlines (≥ 3:1) |
| `fg` | `#1f1d1a` | `#ece8e0` | primary text |
| `fg-muted` | `#5e584f` | `#a9a297` | secondary text |
| `fg-subtle` | `#736c60` | `#8c857a` | tertiary text, icons |
| `accent` | `#0d6b5e` | `#5fc4af` | primary buttons, active marker, focus |
| `accent-fg` | `#ffffff` | `#0a1f1b` | text on accent |
| `accent-soft` / `accent-text` | `#ddeee9` / `#0a5c51` | `#17352f` / `#72d0bc` | selected states, links |
| `highlight` / `mark` | `#f2b84b` / `#fbe7b5` | `#e7a93a` / `#4a3a14` | wordmark dot / search hits |
| `danger` (+`-fg`, `-soft`) | `#b42318` | `#f28b7e` | destructive actions, errors |

### Contrast (WCAG 2.2, verified)

| Pair | Light | Dark | Needs |
|---|---|---|---|
| `fg` on `bg` / `surface` | 15.4 / 16.8 | 15.1 / 14.2 | 4.5 |
| `fg-muted` on `bg` / `surface` / `surface-2` | 6.5 / 7.0 / 6.0 | 7.3 / 6.9 / 6.2 | 4.5 |
| `fg-subtle` on `bg` / `surface` | 4.8 / 5.2 | 5.1 / 4.8 | 4.5 |
| `accent-fg` on `accent` (buttons) | 6.4 | 8.2 | 4.5 |
| `accent-text` on `bg` / `surface` / `accent-soft` | 7.2 / 7.9 / 6.6 | 10.1 / 9.5 / 7.2 | 4.5 |
| `danger` on `bg` / `surface` / `danger-soft` | 6.0 / 6.6 / 5.5 | 7.7 / 7.3 / 6.5 | 4.5 |
| `border-strong` on `surface` (control edges) | 3.3 | 3.2 | 3.0 |
| `accent` on `bg` (focus ring, active marker) | 5.9 | 8.8 | 3.0 |

Re-run the check (a 20-line relative-luminance script) whenever a token changes.

## Themes

- `data-theme="light" | "dark"` on `<html>` selects the palette. `public/theme-init.js`
  (external, because the CSP forbids inline scripts) applies the saved choice before
  first paint; `ThemeProvider` (`src/lib/theme.tsx`) keeps it in sync.
- Preference is `system | light | dark`, stored in `localStorage["akasha-theme"]`
  (`system` = key absent). `system` follows `prefers-color-scheme` live.
- Without JS, a media query applies the dark palette when the system is dark.
- Change it from the sidebar toggle, the user menu, the command palette or Settings.

## Space, shape, depth

- **Spacing**: Tailwind's 4px unit (`--spacing: 0.25rem`). Use the 4/8 rhythm: 1 (4),
  2 (8), 3 (12), 4 (16), 6 (24), 8 (32), 10 (40), 12 (48). Page gutters 16px on phones,
  32–48px on desktop; content max width 1080px; sidebar 248px.
- **Radius**: `xs` 3 (keys) · `sm` 5 (menu items, badges) · `md` 8 (buttons, inputs) ·
  `lg` 12 (cards, menus) · `xl` 18 (dialogs) · `full` (avatars, pills).
- **Elevation**: `shadow-xs` (resting controls) · `sm` (selected segment) · `md` (menus,
  popovers) · `lg` (dialogs, toasts). Light shadows are warm-tinted and faint; in dark mode
  surfaces step lighter instead and shadows only separate overlays.

## Interaction

- **Focus**: every focusable element shows a 2px `--focus` outline with 2px offset on
  `:focus-visible`; inputs use an accent border plus a soft 3px ring instead. Never
  remove focus styles without a replacement.
- **Targets**: at least 36px (`h-9`) for desktop controls, 40–44px for inputs and phone
  targets; the phone tab bar items are 64px tall.
- **Motion tokens**: `--motion-fast` 120ms (hover, menus closing), `--motion-base` 180ms
  (default transition), `--motion-slow` 260ms (dialogs and toasts rising in), easing
  `cubic-bezier(0.2, 0.8, 0.2, 1)`. Under `prefers-reduced-motion: reduce` the tokens
  become 0ms and `animate-*` utilities are disabled.
- **Feedback**: inline errors under fields and a form-level `role="alert"` banner for
  server errors (server messages are sentence-cased); toasts for completed actions that
  have no visible result of their own (saved, password changed, account deleted).
- **Keyboard**: ⌘K / Ctrl+K opens the command palette everywhere in the app; Esc closes
  overlays; a "Skip to content" link is the first tab stop.

## Layout

- **Desktop (≥ 768px)**: sticky left rail (wordmark + theme toggle, "Jump to…" palette
  button, nav, user menu at the bottom) and a centred content column.
- **Phone**: top bar (wordmark, palette, theme, avatar menu) and a bottom tab bar for the
  four destinations, respecting the safe-area inset. Content gets bottom padding so the
  tab bar never covers it.
- **Pages** start with `PageHeader`: eyebrow, serif title, one-line description, actions
  on the right (below on phones), closed by a hairline.
- **Auth** screens: an editorial aside (pull quote, three numbered principles, faint
  concentric rules) beside a narrow form; phones show the quote as a lead-in instead.

## Components

`src/components/ui/`: `button` (primary, secondary, ghost, danger, link; sm/md/lg/icon),
`input`, `label`, `field` (label + input + hint/error with ARIA wiring), `card`, `badge`,
`kbd`, `dialog`, `dropdown-menu`, `tooltip`, `tabs`, `segmented` (single-choice toggle
group), `skeleton`, `toast` (`toast()` callable from anywhere + `<Toaster/>`).
`src/components/common/`: `wordmark`, `page-header`, `empty-state`, `avatar`.
`src/components/shell/`: sidebar, mobile header/tab bar, user menu, theme toggle,
command palette, upload drop zone.

Rules: compose from these before writing new styles; add a variant rather than a
one-off class soup; keep components under ~300 lines.

## Voice

Short, plain, second person. "Nothing remembered yet", not "No data". Name the thing
("Delete forever"), not the mechanism ("Confirm"). Errors say what happened and what to
do next.
