# Getting started

## Install

Add spytial and serde to your `Cargo.toml`:

```toml
[dependencies]
spytial = "0.1"
serde = { version = "1", features = ["derive"] }
```

There's nothing else to install: no system packages, and nothing fetched
at runtime. The HTML template and the rendering JavaScript are compiled
into the crate as `include_str!` payloads, so diagrams work offline. The
minimum supported Rust version is **1.80**.

## Your first diagram

Drop this into `src/main.rs`:

```rust
use spytial::{dbg, SpytialDecorators};
use serde::Serialize;

#[derive(Debug, Serialize, SpytialDecorators)]
#[attribute(field = "key")]
struct Node {
    key: u32,
    left: Option<Box<Node>>,
    right: Option<Box<Node>>,
}

fn main() {
    let tree = Node {
        key: 5,
        left: Some(Box::new(Node { key: 3, left: None, right: None })),
        right: Some(Box::new(Node { key: 7, left: None, right: None })),
    };

    let _ = dbg!(tree);
}
```

Run it:

```sh
cargo run
```

Two things happen:

1. Your terminal prints `[src/main.rs:LINE:COL] tree = Node { … }` —
   exactly what `std::dbg!` would have printed.
2. A browser viewer opens with the rendered tree as capture 1. Later `dbg!`
   calls append captures to the same viewer without opening more tabs.

The three derives are the whole contract: `Debug` (already required by
`std::dbg!`), plus `Serialize` and `SpytialDecorators`. The single
`#[attribute(field = "key")]` decorator promotes each node's `key` into
its label; without it the nodes would be anonymous. The next page,
[Decorators](./decorators.md), covers the rest.

## The entry points

| Call | Behavior |
|------|----------|
| `dbg!(x)` | Prints `[file:line:col] x = {:#?}` to stderr, appends an ordered capture to the process viewer, and returns `x` through. `dbg!(&x)` borrows; `dbg!(a, b)` returns `(a, b)` and appends two captures. |
| `dbg_in!(&session; x)` | The same behavior, routed to an explicit `ViewerSession`. |
| `diagram(&x)` | Writes and opens a standalone diagram, with no stderr or source expression, and doesn't move `x`. |

Decorators on a type apply automatically wherever a value of that type
appears inside another decorated type — the derive walks `Vec<T>`,
`Option<T>`, `Box<T>`, and their nested combinations at compile time. You
never register nested types anywhere.

## The viewer session

The first `dbg!` call creates one default session for the process. Each capture
gets a monotonic sequence number while holding the session lock, so calls from
multiple threads cannot be lost or corrupt the stream. The viewer initially
selects the latest capture; selecting an older capture stops following the
tail until you select the newest one again.

Spytial writes the whole session after every capture to one self-contained HTML
file in your OS temp directory, named like
`spytial-session-{pid}-{counter}-{nanos}.html`. The live browser polls a tiny
in-process HTTP server on `127.0.0.1` using an OS-selected, collision-free port.
The server and live updates end when the process exits, but the HTML snapshot
remains usable afterward and contains the bundled rendering assets.
The transport is intentionally synchronous and single-user; it cannot be
configured to bind beyond loopback and is not a deployment server.

To pin the output to a known path — for serving it from a static file
server, or copying it off a remote machine — set `SPYTIAL_OUTPUT_PATH`:

```sh
SPYTIAL_OUTPUT_PATH=/tmp/my-session.html cargo run
```

The path is read when the session is created, taken verbatim, and atomically
refreshed with the complete capture list. Its parent directory must already
exist. If you create multiple explicit sessions while setting one output path,
they will intentionally target the same file, so give only one of them that
configuration.

To separate capture streams inside one process, create a named session and use
`dbg_in!`:

```rust
use spytial::{dbg_in, ViewerSession};

let parser = ViewerSession::named("parser");
let state = dbg_in!(&parser; state);
```

## Skipping the browser launch

In CI, over SSH, or inside a container — anywhere there's no display —
disable the browser launch with `SPYTIAL_NO_OPEN` (`1`, `true`, or `yes`):

```sh
SPYTIAL_NO_OPEN=1 cargo run
```

stderr is unaffected, so `cargo test` capture behaves exactly as it does
for `std::dbg!`. No viewer server is started. Spytial still appends every
capture to the session HTML and prints that stable path so you can open it
manually after or during the run. See
[Running headless & in Docker](./headless.md) for the full setup.
