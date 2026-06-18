# Soar documentation

Documentation site for [Soar](https://github.com/pkgforge/soar), built with
[VitePress](https://vitepress.dev) and a custom Nightsky theme.

## Local development

```sh
bun install
bun run dev
```

The site is served at `http://localhost:5173`.

## Build

```sh
bun run build
bun run preview
```

The static site is generated into `.vitepress/dist`.

## Structure

- `index.md` is the home page. The hero, install command, and quick navigation
  come from the custom theme in `.vitepress/theme`.
- Page content lives as Markdown files at the docs root, with release notes
  under `releases/`.
- The `/install.sh` path used by the install command is redirected to soar's
  install script by Cloudflare, so it is not served from this site.
- Navigation and sidebar are defined in `.vitepress/config.mts`.
