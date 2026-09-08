# Running headless & in Docker

Spytial's default is to open one persistent browser viewer for `dbg!` captures;
standalone `diagram(&value)` calls still open their own files. Both behaviors
are wrong for CI, remote shells, containers, and test suites. Two environment
variables cover the headless cases.

## `SPYTIAL_NO_OPEN` — suppress the browser launch

Set to `1`, `true`, or `yes` (case-insensitive) and spytial skips the
platform browser-open command. For `dbg!`, it also skips the live loopback
server while continuing to append captures to the session HTML on disk. The
stable path is printed to stderr:

```sh
SPYTIAL_NO_OPEN=1 cargo run --example rbt
```
```text
spytial: capture appended to /tmp/spytial-session-12345-0-987654321.html
```

`dbg!`'s pretty-printed output is unaffected, so `cargo test` capture
behaves exactly as it does for `std::dbg!`.

## `SPYTIAL_OUTPUT_PATH` — pin the output filename

The default `dbg!` session already uses one stable, unique temp path. To choose
it — for serving with a static HTTP server, retaining it as an artifact, or
round-tripping it off a remote machine — set `SPYTIAL_OUTPUT_PATH`:

```sh
SPYTIAL_OUTPUT_PATH=/var/www/diagram.html cargo run
```

The value is read when the session starts and used verbatim. Each capture
atomically replaces it with the complete ordered session, the parent directory
must already exist, and concurrent calls are serialized. The file persists
after process exit. Operating-system cleanup policies eventually remove the
default temp file; Spytial does not delete it because it is the post-mortem
viewer once the in-process server has stopped.

`diagram()` retains its standalone behavior: with this variable set, each
standalone call overwrites the path with one diagram rather than a session.

## Persistent transport limits

The `dbg!` viewer transport is a synchronous, single-user debugging aid. It is
not configurable to listen beyond `127.0.0.1`, has no external-service or
deployment mode, and stops with the captured program. This is separate from
the repository's `viz_server` demo binary and deliberately does not turn that
server into the async, authenticated deployment service discussed in issue
#47.

## Docker

The repository ships a `Dockerfile` and entrypoint that give you a
one-command demo without a local Rust toolchain:

```sh
docker build -t spytial .
docker run --rm -p 8080:8080 spytial        # default: the rbt example
docker run --rm -p 8080:8080 spytial demo   # any example in examples/
```

The container builds and runs the example once, then serves the rendered
HTML over a small HTTP server. Open <http://localhost:8080/rust_viz_data.html>
on your host. The image sets both variables above — `SPYTIAL_NO_OPEN=1`
via the Dockerfile (no display in the container) and
`SPYTIAL_OUTPUT_PATH=/tmp/rust_viz_data.html` via the entrypoint (the path
the server serves). The server is single-threaded and for local viewing
only.
