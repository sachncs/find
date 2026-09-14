# find — product site

The marketing site for [`find`](https://github.com/sachncs/find), a high-performance Rust
engine for secp256k1 scalar discovery. Built with Astro 5, React islands, and Tailwind v4.

## Stack

| Layer        | Tooling                          |
| ------------ | -------------------------------- |
| Framework    | [Astro](https://astro.build) 5 (static output) |
| UI islands   | React 19 + Framer Motion         |
| Styling      | Tailwind CSS v4 (`@tailwindcss/vite`) |
| Type system  | TypeScript (strict)              |
| Sitemap      | `@astrojs/sitemap`               |
| Fonts        | Inter (UI) + JetBrains Mono (code), self-loaded from Google Fonts |

## Develop

```bash
pnpm install
pnpm dev          # http://localhost:4321
```

## Build

```bash
pnpm build        # outputs to ./dist
pnpm preview      # preview the built site locally
```

The base path is `/find` (GitHub Pages project pages). The CI workflow at
`../.github/workflows/docs.yml` builds `site/dist` and uploads it as the Pages artifact.

## Structure

```
site/
├── astro.config.mjs      # base path, integrations, Vite plugins
├── tailwind              # configured via @tailwindcss/vite in astro.config.mjs
├── public/               # static assets served as-is (favicon, og.svg, robots.txt)
└── src/
    ├── styles/global.css # design tokens + Tailwind theme + component primitives
    ├── layouts/Base.astro
    ├── components/       # Astro server components + React island(s)
    └── pages/index.astro
```

## Conventions

- **No markdown rendering.** Copy lives as Astro/TSX — not generated from `docs/`.
- **Dark-first.** Default theme is dark for the cinematic hero; respects `prefers-color-scheme`.
- **Reduced motion.** All animations respect `prefers-reduced-motion: reduce`.
- **No unsafe in the UI.** The hot path stays Rust's problem.