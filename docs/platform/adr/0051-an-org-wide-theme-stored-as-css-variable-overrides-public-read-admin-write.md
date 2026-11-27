# An org-wide Theme, stored as CSS-variable overrides: public to read, admins' to change

An organisation running an app built from the template wants it to look like theirs: its colours, its fonts, its corners. Until now that meant editing the SPA's stylesheet and rebuilding, which a non-technical admin can't do, and which ADR 0005's one artifact would have to be rebuilt for.

**A platform module, `appearance`, stores one Theme for the whole deployment.** It is overrides of the SPA's CSS variables (its design tokens), by name without the `--`, in three maps: `light` and `dark` for the colour schemes, and `shared` for what both use (radius, fonts, shadows). Empty maps are the defaults, so saving them resets the Theme. It is one aggregate root keyed `org`, absent until first saved (version `0`), versioned and announced on `appearance.theme.v1` like any root (ADR 0016).

**Reading is public; changing it is admins'.** `get_theme` is in `PUBLIC` (ADR 0040): the SPA applies the Theme before sign-in, on the sign-in screen too, and it holds nothing but CSS values. `update_theme` takes `If-Match` (ADR 0030) and is guarded by `theme` / `update`, `admin`'s by default grant with no role of its own (ADR 0041).

**Applied at runtime, with no rebuild.** The SPA fetches the Theme and writes the overrides as a stylesheet of custom properties over its defaults. So a change shows on the next load, in the one artifact already deployed.

**Validated because it is injected into a stylesheet.** Names must be in a closed allowlist per map (the tokens the SPA defines), and values non-empty, at most 200 characters, and free of `;`, `{`, `}`, `<`, `>`, `\` and `url(`: nothing that could end a declaration or rule, close the `<style>`, escape, or fetch from elsewhere. A bad one is `400` naming its path (`light.primary`).

## Considered options

- **Build-time theming (a config file or env var read by Vite).** Each change needs a rebuild and a deploy, which only a developer can do.
- **Free-form CSS.** Hard to make safe to inject, and it couples admins' CSS to the SPA's markup. Variables are the SPA's public styling contract.
- **A Theme per user or per Capability.** No one asked for it; one org-wide Theme keeps the model one row. Per-user light or dark choice stays in the browser.
- **Guarded reads.** The SPA would show its default theme until sign-in, then flash to the org's.

## Consequences

- A token the SPA renames or removes leaves stored overrides for it inert; the allowlist and the SPA's tokens change together.
- Values aren't parsed as CSS: a nonsensical one (`primary: banana`) is stored and ignored by the browser.
- Logos, favicons and the app's name are not part of the Theme. They would need file storage and are a follow-up.
