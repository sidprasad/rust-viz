# Vendored spytial-core assets

These files are vendored from [spytial-core](https://github.com/sidprasad/spytial-core)
at the version recorded in `VERSION.txt`. The four browser assets are bundled into the
rendered HTML by `src/lib.rs` at compile time via `include_str!`, so `dbg!`/`diagram`
works offline and without network access.

To update, run the script — don't copy files by hand:

```bash
scripts/update-spytial-core.sh 4.3.0
```

It pulls the published tarball (a local `npm run build:all` produces the same bytes,
but the tarball is what consumers actually get), copies the files below, rewrites
`VERSION.txt`, and regenerates `macros/src/spec_tables.rs` from the new manifest.

That last step is why the script exists rather than a checklist: the derive macro's
accepted keys and compile-time validation are *derived* from `spytial-language.json`,
so bumping the version without regenerating leaves the macro describing the previous
language. `spec-codegen`'s drift test catches it in CI, but the script is what keeps
the two in step to begin with.

| File in this directory | Source in spytial-core |
|---|---|
| `spytial-core.global.js` | `dist/browser/spytial-core-complete.global.js` |
| `spytial-core.css` | `dist/browser/spytial-core-complete.css` |
| `react-component-integration.global.js` | `dist/components/react-component-integration.global.js` |
| `react-component-integration.css` | `dist/components/react-component-integration.css` |
| `spytial-language.json` | `docs/spytial-language.json` |

`.map` files are intentionally not vendored to keep the published crate small.

## `spytial-language.json`

Unlike the four assets above, the manifest is never served to a browser. It is the
machine-readable description of the layout-spec language — every constraint and
directive, its fields, their closed vocabularies, numeric bounds, and which of them
the engine actually rejects versus silently ignores (`enforcement`).

`spec-codegen/` reads it to generate `macros/src/spec_tables.rs`, so the derive
macro's accepted keys and compile-time validation are derived from spytial-core's
own description of the language rather than transcribed by hand. Bumping the vendored
version and forgetting to regenerate is caught by a test in that crate.

First shipped in spytial-core 4.3.0; there is no equivalent file in 4.1.0 or earlier.

## What we deliberately don't vendor

spytial-core 4.0.0 split two optional surfaces out of the CDN main global, each
into its own script: `dist/browser/spytial-core-sql.global.js` (`SQLEvaluator`) and
`dist/browser/spytial-core-explorer.global.js` (`<spytial-explorer>`). A page that
wants them loads the extra script after the main one.

`templates/template.html` needs neither: it evaluates selectors with
`SGraphQueryEvaluator`, and it renders through `<webcola-cnd-graph>`, which the main
global still registers. Adding either script would grow every generated page by
~0.4-0.5 MB for code the page never calls.
