# The SPA is a suite of apps in one shell, each with a few areas

The template is a megalith: one deployment that grows many products (a CRM, timesheets, billing, rostering), each with a few functions of its own (CRM › Contacts, Deals). The SPA had no shell to hold them: a right-aligned row of text links in the header (Assistant, Access, the user, the theme), `/` was the notes list, every page was a narrow centred column, and `access` split its functions into `?tab=` tabs while notes linked its search as a separate route. Nothing said which product the user was in, and the header couldn't take more than a handful of destinations.

**One shell, two levels.** Every screen inside the suite sits in one shell (`frontend/src/shell/`): a sidebar, a header over the page, and the assistant beside it. Navigation has two levels and no more: an **App**, and its **Areas**, each Area a route at `/<app>/<area>`. A record inside an Area (one role, one invoice) is a page under the Area's path, the Area a link back in the breadcrumb, never a third level of navigation.

**A registry, not hand-built navigation.** Each App is one entry in `src/shell/apps.ts`: a label, a one-line description, an icon, its path and its Areas in order. `useShellApps` returns those the caller may open, leaving out one they may not use by asking the server, as the access screen already did (the SPA reads no roles from its token). From that one list the shell draws the launcher at `/` (a card per App), the sidebar (a switcher over the open App's Areas; collapsible to icons, a sheet on a phone), the breadcrumb (App › Area) and Go to (a ⌘K palette over every Area of every App). Adding an App adds its routes and its entry; the shell is not edited. The platform's own screens (`access`) are entries too, kept apart: in the sidebar's footer and in their own group on the launcher, not among the suite's Apps.

**Areas are routes, never tabs.** `access`'s Users, Roles and Permissions moved from `?tab=` to `/access/subjects`, `/access/roles` and `/access/permissions`, and notes moved to `/notes` and `/notes/search`, so every Area is a link the sidebar can mark, the breadcrumb can name and a user can share. The old URLs redirect, keeping their search.

**The assistant is docked, not a destination.** The assistant's toggle in the header opens its chat as a panel beside whichever App is open (a sheet on a phone), on its own conversation across screens, so the user can ask about what they see; `/assistant` stays as the full page. A write it runs already refreshes every query, so the screen beside it shows the change.

**One look for the suite.** shadcn/ui's neutral greys with one accent (indigo) on what acts or is current; an App is told apart by its icon, never a colour of its own. Dense, light by default following the system, dark kept; every page sits in `Page` (title, a line on what it is for, its primary actions on the right).

## Considered options

- **One tree sidebar**, every App a collapsible group of its Areas. Everything in view at once, but it grows past use beyond about eight Apps, and a user working in one App scrolls past the rest.
- **A top bar of Apps over a sidebar of Areas** (Office-style). Familiar, but the bar overflows with many Apps and has no room on a phone.
- **A switcher over the open App's sidebar** (chosen). Scales to any number of Apps, keeps each App focused, and the launcher and Go to make every App two keystrokes away.
- **A colour per App** for wayfinding. Rejected: it multiplies the palette to check for contrast in light and dark, and the icon, label and breadcrumb already say where the user is.
- **The assistant as a page only.** Simpler, but the user leaves what they are asking about.

## Consequences

- A screen added to an App is a route under its path and, if it is a new Area, a line in its entry; a new App is a line in `useShellApps` and, if some callers may not use it, a hook asking the server whether they may.
- Pages are thin and uniform: `Page` gives each its heading and actions, and none renders `<main>`, navigation or a header.
- Moving a route means a redirect from the old one, so links and bookmarks keep working (`/home`, `/search`, `/access?tab=`).
- `make init` keeps the shell and the platform's tools and empties the suite's Apps: the launcher says there are none yet.
- jsdom lacks `matchMedia`, `ResizeObserver` and `scrollIntoView`, which the sidebar and Go to use; `vitest.setup.ts` stands in for them.
