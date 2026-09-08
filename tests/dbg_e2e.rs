//! End-to-end tests for the `spytial::dbg!` macro and the supporting
//! `diagram` / `export_json_instance` / `try_export_json_instance` surface.
//!
//! These tests lock in the `std::dbg!`-parity contract (move/borrow forms,
//! tuple return, empty form) and the silent-degrade behavior of the export
//! pipeline. They MUST set `SPYTIAL_NO_OPEN=1` so CI/headless runs don't
//! try to open a browser.

use serde::Serialize;
use spytial::export::try_export_json_instance;
use spytial::{dbg, dbg_in, diagram, export_json_instance, SpytialDecorators, ViewerSession};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::thread;
use std::time::SystemTime;

/// Suppress browser launch for every test in this file. `SPYTIAL_NO_OPEN`
/// is read by both output paths; setting it to "1" makes `dbg!` write its
/// persistent session file without starting the loopback server and makes
/// `diagram` write its standalone file without calling `open`/`xdg-open`.
///
/// The env var is process-global and we never unset it, so it's safe for
/// parallel tests to all set the same value.
fn suppress_browser_open() {
    env::set_var("SPYTIAL_NO_OPEN", "1");
}

/// Cross-test coordination for any test that calls `dbg!`/`diagram`.
///
/// `SPYTIAL_OUTPUT_PATH` is process-global, so when
/// `diagram_writes_html_file` sets it, concurrent standalone output could
/// clobber the marker. The default viewer also reads the variable once on its
/// first capture, so it must not be initialized inside another test's override
/// window. Every test that renders (directly or through a debug macro) acquires
/// this mutex for the duration of its output-producing work.
///
/// We use `unwrap_or_else(PoisonError::into_inner)` so one failed test
/// does not cascade into spurious failures of the rest.
fn diagram_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

/// Build a unique tempfile path for routing `diagram()` output during a
/// single test. Uses pid + monotonic counter + nanos so concurrent test
/// binaries on the same machine never collide.
fn unique_output_path(tag: &str) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let pid = std::process::id();
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    env::temp_dir().join(format!("spytial-e2e-{tag}-{pid}-{counter}-{nanos}.html"))
}

// ──────────────────────────────────────────────
// 1. dbg!(x) returns the value (move form)
// ──────────────────────────────────────────────

#[test]
fn dbg_returns_value() {
    suppress_browser_open();

    #[derive(Debug, Serialize, SpytialDecorators)]
    struct W(i32);

    let _guard = diagram_lock();
    let y = dbg!(W(42));
    assert_eq!(y.0, 42, "dbg!(W(42)) must return the value through");
}

// ──────────────────────────────────────────────
// 2. dbg!(&x) does not move x
// ──────────────────────────────────────────────

#[test]
fn dbg_borrow_form_does_not_move() {
    suppress_browser_open();

    // !Copy struct: owns a heap allocation so the compiler will reject any
    // accidental move.
    #[derive(Debug, Serialize, SpytialDecorators)]
    struct NoCopy {
        payload: String,
    }

    let value = NoCopy {
        payload: "still-alive".to_string(),
    };

    {
        let _guard = diagram_lock();
        let _borrowed = dbg!(&value);
    }

    // If dbg!(&value) moved `value`, this line would not compile.
    assert_eq!(value.payload, "still-alive");
}

// ──────────────────────────────────────────────
// 3. dbg!(a, b) returns a tuple
// ──────────────────────────────────────────────

#[test]
fn dbg_tuple_form_returns_tuple() {
    suppress_browser_open();

    #[derive(Debug, Serialize, SpytialDecorators)]
    struct W(i32);

    let _guard = diagram_lock();
    let (a, b) = dbg!(W(1), W(2));
    assert_eq!(a.0, 1);
    assert_eq!(b.0, 2);
}

#[test]
fn named_session_records_multi_argument_captures_and_metadata() {
    suppress_browser_open();

    #[derive(Debug, Serialize, SpytialDecorators)]
    struct W(i32);

    let target = unique_output_path("named-session");
    let _guard = diagram_lock();
    env::set_var("SPYTIAL_OUTPUT_PATH", &target);

    let session = ViewerSession::named("parser");
    let (first, second) = dbg_in!(&session; W(10), W(20));
    let read_result = fs::read_to_string(&target);
    env::remove_var("SPYTIAL_OUTPUT_PATH");
    drop(_guard);

    assert_eq!((first.0, second.0), (10, 20));
    let contents = read_result.unwrap_or_else(|error| {
        panic!(
            "named session should write {}; could not read: {error}",
            target.display()
        )
    });
    let first_position = contents.find("W(10)").expect("first expression metadata");
    let second_position = contents.find("W(20)").expect("second expression metadata");
    assert!(
        first_position < second_position,
        "captures should stay ordered"
    );
    assert!(
        contents.contains("parser"),
        "session name should be embedded"
    );
    let escaped_source_file = serde_json::to_string(file!()).expect("serialize source filename");
    assert!(
        contents.contains(escaped_source_file.trim_matches('"')),
        "source filename should be embedded"
    );
    assert!(contents.contains("timestamp_unix_ms"));
    assert!(contents.contains("ThreadId("));

    let _ = fs::remove_file(target);
}

// ──────────────────────────────────────────────
// 4. dbg! on a zero-field value does not panic
// ──────────────────────────────────────────────

#[test]
fn dbg_does_not_panic_on_empty_struct() {
    suppress_browser_open();

    #[derive(Debug, Serialize, SpytialDecorators)]
    struct EmptyFields {}

    #[derive(Debug, Serialize, SpytialDecorators)]
    struct Unit;

    let _guard = diagram_lock();
    // Just confirm the dbg! invocations run to completion without
    // panicking. We don't care about the contents of the diagram, only
    // that the path through the macro + diagram + export pipeline is
    // panic-free.
    let _ = dbg!(EmptyFields {});
    let _ = dbg!(Unit);
}

// ──────────────────────────────────────────────
// 5. export_json_instance handles deeply-nested Options
// ──────────────────────────────────────────────

#[test]
fn export_json_instance_handles_nested_options() {
    suppress_browser_open();

    #[derive(Debug, Serialize, SpytialDecorators)]
    struct Inner {
        v: u32,
    }

    #[derive(Debug, Serialize, SpytialDecorators)]
    struct Nested {
        deeply: Option<Option<Box<Inner>>>,
    }

    // Some(Some(Box::new(Inner { v: 7 })))
    let value = Nested {
        deeply: Some(Some(Box::new(Inner { v: 7 }))),
    };
    let inst = export_json_instance(&value);

    assert!(!inst.atoms.is_empty(), "expected non-empty atoms");
    assert!(!inst.relations.is_empty(), "expected non-empty relations");

    // The inner u32 value should make it through unwrapping.
    assert!(
        inst.atoms.iter().any(|a| a.label == "7"),
        "expected an atom with label \"7\" reflecting Inner.v"
    );

    // Both struct types should appear as atom types.
    assert!(inst.atoms.iter().any(|a| a.r#type == "Nested"));
    assert!(inst.atoms.iter().any(|a| a.r#type == "Inner"));

    // None at the outer-Option level should also serialize cleanly.
    let none_value = Nested { deeply: None };
    let none_inst = export_json_instance(&none_value);
    assert!(!none_inst.atoms.is_empty());
    assert!(none_inst.atoms.iter().any(|a| a.r#type == "None"));
}

// ──────────────────────────────────────────────
// 6. try_export_json_instance returns Ok on a normal value
// ──────────────────────────────────────────────

#[test]
fn try_export_json_instance_returns_ok_on_valid_value() {
    suppress_browser_open();

    #[derive(Debug, Serialize, SpytialDecorators)]
    struct Simple {
        x: i32,
        y: String,
    }

    let value = Simple {
        x: 7,
        y: "hello".to_string(),
    };

    let result = try_export_json_instance(&value);
    assert!(
        result.is_ok(),
        "expected Ok for a normal value, got {:?}",
        result
    );

    let inst = result.unwrap();
    assert!(!inst.atoms.is_empty());
    assert!(inst.atoms.iter().any(|a| a.r#type == "Simple"));
}

// ──────────────────────────────────────────────
// 7. diagram() writes the HTML file with the JSON data inside
//
// `diagram` now picks a unique tempfile path per call (pid + counter +
// nanos), so this test routes its output to a known location by setting
// `SPYTIAL_OUTPUT_PATH`.  Other tests would pick up that override too,
// so this test holds `diagram_lock` to keep them out of its write/read
// window.
// ──────────────────────────────────────────────

#[test]
fn diagram_writes_html_file() {
    suppress_browser_open();

    #[derive(Debug, Serialize, SpytialDecorators)]
    struct Marker {
        unique_marker_field: String,
    }

    let value = Marker {
        unique_marker_field: "spytial-dbg-e2e-marker-XYZ123".to_string(),
    };
    let target = unique_output_path("diagram-html");

    let _guard = diagram_lock();
    env::set_var("SPYTIAL_OUTPUT_PATH", &target);

    diagram(&value);

    // Snapshot the file contents while the env var is still set so no
    // other concurrent test can route a different write to `target`.
    let read_result = fs::read_to_string(&target);

    // Restore the env so the rest of the suite goes back to per-call
    // unique paths.
    env::remove_var("SPYTIAL_OUTPUT_PATH");
    drop(_guard);

    let contents = read_result.unwrap_or_else(|err| {
        panic!(
            "diagram() should write {}; could not read: {err}",
            target.display()
        )
    });

    // The unique field value should be embedded as a string atom label
    // in the JSON payload baked into the HTML.
    assert!(
        contents.contains("spytial-dbg-e2e-marker-XYZ123"),
        "rendered HTML at {} should contain the unique marker value",
        target.display(),
    );
    // And the struct type name should appear as an atom type.
    assert!(
        contents.contains("Marker"),
        "rendered HTML at {} should reference the struct type name",
        target.display(),
    );

    // Best-effort cleanup.
    let _ = fs::remove_file(&target);
}

// ──────────────────────────────────────────────
// 8. Concurrent dbg! calls do not panic
//
// 4 threads each call `dbg!(W(i))` 5 times. None should panic or lose its
// returned value while the default session serializes their captures into one
// ordered stream. The focused session unit test also inspects that stream for
// contiguous sequence numbers and the expected capture count.
//
// The outer test fn holds `diagram_lock` so it doesn't race with
// `diagram_writes_html_file` setting `SPYTIAL_OUTPUT_PATH`.  The
// per-thread `dbg!` calls do NOT take the lock — that's the point of
// the test.
// ──────────────────────────────────────────────

#[test]
fn concurrent_dbg_calls_do_not_panic() {
    suppress_browser_open();

    #[derive(Debug, Serialize, SpytialDecorators)]
    struct W(i32);

    let _guard = diagram_lock();

    let panics = Arc::new(Mutex::new(Vec::<String>::new()));

    let mut handles = Vec::new();
    for t in 0..4 {
        let panics = Arc::clone(&panics);
        handles.push(thread::spawn(move || {
            // Catch any panic from inside the closure so the parent test
            // can surface a meaningful assertion message.
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                for i in 0..5 {
                    let val = t * 100 + i;
                    let returned = dbg!(W(val));
                    // dbg! must always return the value through, even
                    // under concurrent file-write collisions.
                    assert_eq!(returned.0, val);
                }
            }));
            if let Err(payload) = result {
                let msg = if let Some(s) = payload.downcast_ref::<&str>() {
                    (*s).to_string()
                } else if let Some(s) = payload.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "non-string panic payload".to_string()
                };
                panics.lock().unwrap().push(format!("thread {t}: {msg}"));
            }
        }));
    }

    for h in handles {
        h.join().expect("thread join failed");
    }

    let panics = panics.lock().unwrap_or_else(PoisonError::into_inner);
    assert!(
        panics.is_empty(),
        "no thread should panic under concurrent dbg!, got: {:?}",
        *panics
    );
}
