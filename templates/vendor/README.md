# Vendored spytial-core assets

These files are vendored from [spytial-core](https://github.com/sidprasad/spytial-core)
at the version recorded in `VERSION.txt`. They are bundled into the rendered HTML by
`src/lib.rs` at compile time via `include_str!`, so `dbg!`/`diagram` works offline and
without network access.

To update: `npm pack spytial-core@<version>` and copy the four files in this directory
out of the tarball's `package/dist/` (a local `npm run build:all` produces the same
bytes, but the tarball is what consumers actually get). Update `VERSION.txt` to match.

| File in this directory | Source in spytial-core |
|---|---|
| `spytial-core.global.js` | `dist/browser/spytial-core-complete.global.js` |
| `spytial-core.css` | `dist/browser/spytial-core-complete.css` |
| `react-component-integration.global.js` | `dist/components/react-component-integration.global.js` |
| `react-component-integration.css` | `dist/components/react-component-integration.css` |

`.map` files are intentionally not vendored to keep the published crate small.

## What we deliberately don't vendor

spytial-core 4.0.0 split two optional surfaces out of the CDN main global, each
into its own script: `dist/browser/spytial-core-sql.global.js` (`SQLEvaluator`) and
`dist/browser/spytial-core-explorer.global.js` (`<spytial-explorer>`). A page that
wants them loads the extra script after the main one.

`templates/template.html` needs neither: it evaluates selectors with
`SGraphQueryEvaluator`, and it renders through `<webcola-cnd-graph>`, which the main
global still registers. Adding either script would grow every generated page by
~0.4-0.5 MB for code the page never calls.
