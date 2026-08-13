# Design - any-watch

This is the locked design system for the `any-watch` replacement. Every new
screen and component follows it. The existing `any-watch` visual system remains
legacy reference material only.

## Product context

- Audience: authenticated family members using desktop, mobile, and TV browsers.
- Primary job: resume a title or choose a verified viewing option with minimal
  interruption.
- Voice: cinematic, quiet, deliberate, and operationally honest.
- Brand: `any-watch`; a small luminous mark on a dark field, never a borrowed
  platform logo or visual clone.

## Design direction

The experience is Apple-inspired in its material restraint, hierarchy, focus,
motion, and control density. It is original in layout, components, typography,
and interaction. It is not a reproduction of Apple or Netflix interfaces.

Genre: atmospheric cinema with a precise, premium product surface. Glass is a
contextual interaction material for navigation, menus, controls, and sheets. It
does not sit over dense poster art or reduce playback readability.

## Application structure

- Login: **Private Screening**. One quiet authentication surface beside a
  cinematic still or abstract light field. No marketing claims or public signup.
- Home: **Viewing Room**. Continue Watching has first visual priority, followed
  by unequal title rails rather than a uniform card grid.
- Search and title detail: **Title Observatory**. Canonical title information is
  primary; distinct viewing options remain explicit and source-scoped.
- Player: **Theatre Controls**. Content first, then a compact layered control
  system and one contextual settings sheet.
- Settings: **Control Cabinet**. Playback, language, accessibility, TV, and
  administrator account management are grouped by task.

Desktop navigation uses a compact edge rail that collapses into an icon strip.
Mobile uses a safe-area bottom bar with at most five destinations. TV uses the
same information architecture with a focus-first rail and no hover-only action.
Application screens do not use marketing footers.

## Theme - Midnight Prism

- `--color-paper`: `oklch(10% 0.018 260)`
- `--color-paper-2`: `oklch(15% 0.020 260)`
- `--color-paper-3`: `oklch(21% 0.024 260)`
- `--color-ink`: `oklch(97% 0.008 260)`
- `--color-ink-2`: `oklch(78% 0.014 260)`
- `--color-muted`: `oklch(61% 0.018 260)`
- `--color-rule`: `oklch(32% 0.026 260)`
- `--color-rule-strong`: `oklch(45% 0.030 260)`
- `--color-accent`: `oklch(74% 0.16 205)`
- `--color-accent-ink`: `oklch(16% 0.024 260)`
- `--color-focus`: `oklch(82% 0.14 205)`
- `--color-danger`: `oklch(66% 0.21 25)`

Theme axes: **dark / grotesk-sans / cool**. The cyan-blue accent identifies
primary play/resume actions, focus, active progress, and the small brand mark.
Warm color is reserved for destructive or warning states. Provider colors must
not replace the product accent or dominate the UI.

Variants:

- **Midnight Prism**: default.
- **OLED Theatre**: lowers surfaces for dark rooms without pure black.
- **Device Contrast**: increases rules and muted-text separation when requested.

Variants change token values only; they never alter navigation, typography,
source status language, or interaction semantics.

## Typography

- Display: `Manrope`, weight 700/800, roman.
- Body: `Manrope`, weight 400/500/650, roman.
- Mono: `IBM Plex Mono`, weight 400/500.
- Display tracking: `-0.03em`; headings never use italics.
- Use 45-70 character measures for prose and denser measures only for controls.
- Bundle fonts with the application. Do not rely on external font CDNs.

## Space and motion

- Use a named 4-point spacing scale and semantic layout tokens.
- Large-screen surfaces use generous negative space; dense data lives in sheets
  and panels, not floating card collections.
- Route changes use opacity and short transform transitions only.
- State changes use 140-220 ms motion with no bounce or continuous ambient loop.
- Reduced motion is opacity-only at 150 ms or less.

## Interaction rules

- Every control has visible default, hover, focus-visible, active, disabled,
  loading, error, and success states.
- Focus appears immediately, has strong contrast, and is enlarged for TV mode.
- Never hide essential poster labels, provider status, or source selection behind
  hover.
- Provider status is explicit: `healthy`, initially `unknown`, or retryable
  `unavailable`; a failed aggregate check never leaves providers indefinitely
  in `Checking`.
- A source choice is always intentional. Fallback suggestions explain why and
  require user selection.
- Saves are silent. Destructive library actions offer Undo when reversible.

## Screen rules

### Login

Keep username/password login and a concise family-access explanation. Do not
imply OAuth, account recovery, public registration, or social features.

### Home and discovery

Continue Watching leads when it exists. Show language context and source
availability at the title level. Use full-bleed artwork sparingly and always
preserve readable metadata.

### Search and detail

Search begins from a title. Provider-specific results and viewing options retain
their own labels, capabilities, language, and health state. Switching a filter
preserves the query. Long episode lists use 50-episode ranges and exact episode
numbers.

### Player

The player is distraction-free. Provider, quality, subtitles, speed, playback
settings, and fallback choices live in one contextual sheet. Progress, resume,
intro/ending markers, Skip intro, and next episode remain keyboard, touch, and remote usable.
Opening and ending ranges use distinct semantic timeline bands. A compact
lower-right Theatre Controls card exposes the persistent Skip intro switch; the
markers remain visible when Skip intro is off so the viewer retains context.

### Settings

Expose language, subtitle defaults, quality, autoplay, skip behavior, contrast,
motion, text scale, TV mode, source ordering, and protected administrator user
management. Free adjustment must not expose server secrets or provider tokens.

## Responsive and accessibility contract

Verify 320, 375, 414, 768, 1100, 1440, and 1728 px widths plus TV viewports.
There is no horizontal page scroll. Touch targets are at least 44 by 44 px.
Image grids use `minmax(0, 1fr)`. Text buttons and primary navigation labels do
not wrap. Desktop hover behavior always has touch and remote equivalents.

## Security contract

- Hosted access is authenticated and account data is per-user.
- Production cookies are Secure, HttpOnly, SameSite-protected, and same-origin.
- The raw application port is never public.
- Provider credentials, protocol constants, cookies, headers, and signed media
  URLs never enter client code or logs.
- Source health must be honest; no title claims playback until the provider has
  passed current certification.

## Hallmark implementation note

Future screen builds use this locked system rather than catalog rotation. Each
new design artifact records its macrostructure and accessibility verification,
uses named tokens only, and validates the responsive contract above.
