//! End-to-end tests for the `spytial::dbg!` macro and the supporting
//! `diagram` / `export_json_instance` / `try_export_json_instance` surface.
//!
//! These tests lock in the `std::dbg!`-parity contract (move/borrow forms,
//! tuple return, empty form) and the silent-degrade behavior of the export
//! pipeline. They MUST set `SPYTIAL_NO_OPEN=1` so CI/headless runs don't
//! try to open a browser.

use serde::Serialize;
use spytial::export::try_export_json_instance;
use spytial::jsondata::JsonDataInstance;
use spytial::spytial_annotations::HasSpytialDecorators;
use spytial::{dbg, dbg_in, diagram, export_json_instance, SpytialDecorators, ViewerSession};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::thread;
use std::time::SystemTime;

#[derive(Debug, Serialize)]
struct UndecoratedUser {
    name: String,
}

#[derive(Debug, Serialize, SpytialDecorators)]
#[serde(rename = "SerializedRegisteredChild")]
#[attribute(field = "nested_registration_marker")]
struct RegisteredChild {
    nested_registration_marker: String,
}

#[derive(Debug, Serialize, SpytialDecorators)]
#[serde(rename(
    serialize = "SerializedRegisteredParent",
    deserialize = "RegisteredParent"
))]
#[hide_field(field = "parent_registration_marker")]
#[hide_atom(selector = "ROOT_REGISTRATION_SELECTOR")]
struct RegisteredParent {
    parent_registration_marker: String,
    child: Option<RegisteredChild>,
}

#[derive(Debug, Serialize, SpytialDecorators)]
#[serde(transparent)]
#[hide_atom(selector = "TRANSPARENT_ROOT_SELECTOR")]
struct TransparentRoot(String);

#[derive(Debug, Serialize, SpytialDecorators)]
#[serde(untagged)]
#[hide_atom(selector = "UNTAGGED_ROOT_SELECTOR")]
#[allow(dead_code)]
enum UntaggedRoot {
    Text(String),
    Number(i32),
}

mod collision_left {
    use super::*;

    #[derive(Debug, Serialize, SpytialDecorators)]
    #[hide_atom(selector = "LEFT_ITEM_SELECTOR")]
    pub struct Item {
        pub value: String,
    }
}

mod collision_right {
    use super::*;

    #[derive(Debug, Serialize, SpytialDecorators)]
    #[hide_atom(selector = "RIGHT_ITEM_SELECTOR")]
    pub struct Item {
        pub value: String,
    }
}

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

/// Parse the exact datum embedded in the self-contained HTML that the public
/// diagram path writes. This checks the hand-off to the browser, not merely a
/// separate call to `export_json_instance`.
fn embedded_datum(contents: &str) -> JsonDataInstance {
    let (_, after_marker) = contents
        .split_once("const jsonData = `")
        .expect("rendered HTML should declare its JSON datum");
    let (json, _) = after_marker
        .split_once("`;")
        .expect("rendered HTML should terminate its JSON datum");
    serde_json::from_str(json).expect("embedded diagram datum should be valid JSON")
}

/// Parse the capture envelopes embedded in a persistent viewer snapshot.
fn embedded_captures(contents: &str) -> Vec<serde_json::Value> {
    let (_, after_marker) = contents
        .split_once("const initialCaptures = JSON.parse(")
        .expect("session HTML should declare its initial captures");
    let (literal, _) = after_marker
        .split_once(");")
        .expect("session HTML should terminate its initial captures");
    let json: String =
        serde_json::from_str(literal).expect("initial captures should be a JSON string literal");
    serde_json::from_str(&json).expect("embedded captures should be valid JSON")
}

fn capture_datum(capture: &serde_json::Value) -> JsonDataInstance {
    serde_json::from_value(capture["datum"].clone())
        .expect("capture datum should use the JsonDataInstance shape")
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

#[test]
fn dbg_accepts_direct_vec_and_hash_map_values() {
    suppress_browser_open();

    let target = unique_output_path("collections-datum");
    let _guard = diagram_lock();
    env::set_var("SPYTIAL_OUTPUT_PATH", &target);
    let session = ViewerSession::named("collection-datum-test");

    let values = vec![1, 2, 3];
    // The default macro accepts the direct collection; the explicit session
    // then gives this test a deterministic snapshot to inspect.
    let public_returned_values = dbg!(&values);
    assert_eq!(public_returned_values, &vec![1, 2, 3]);
    let returned_values = dbg_in!(&session; &values);
    assert_eq!(returned_values, &vec![1, 2, 3]);
    let captures = embedded_captures(
        &fs::read_to_string(&target).expect("dbg_in! should write the Vec session HTML"),
    );
    assert_eq!(captures.len(), 1);
    let sequence = capture_datum(&captures[0]);
    assert_eq!(sequence.atoms[0].r#type, "sequence");
    assert!(sequence
        .atoms
        .iter()
        .any(|atom| atom.r#type == "i32" && atom.label == "3"));
    assert_eq!(
        sequence
            .relations
            .iter()
            .find(|relation| relation.name == "idx")
            .expect("Vec datum should contain an idx relation")
            .tuples
            .len(),
        3
    );

    let users = HashMap::from([("Ada", 1), ("Grace", 2)]);
    let returned_users = dbg_in!(&session; &users);
    assert_eq!(returned_users.get("Ada"), Some(&1));
    assert_eq!(returned_users.get("Grace"), Some(&2));
    let captures = embedded_captures(
        &fs::read_to_string(&target).expect("dbg_in! should update the HashMap session HTML"),
    );
    assert_eq!(captures.len(), 2);
    let map = capture_datum(&captures[1]);
    assert_eq!(map.atoms[0].r#type, "map");
    assert!(map.atoms.iter().any(|atom| atom.label == "Ada"));
    assert!(map.atoms.iter().any(|atom| atom.label == "Grace"));
    assert_eq!(
        map.relations
            .iter()
            .find(|relation| relation.name == "map_entry")
            .expect("HashMap datum should contain a map_entry relation")
            .tuples
            .len(),
        2
    );

    env::remove_var("SPYTIAL_OUTPUT_PATH");
    drop(_guard);
    let _ = fs::remove_file(&target);
}

#[test]
fn undecorated_user_type_works_with_dbg_and_diagram() {
    suppress_browser_open();

    let value = UndecoratedUser {
        name: "Ada".to_string(),
    };
    let target = unique_output_path("undecorated");

    let _guard = diagram_lock();
    env::set_var("SPYTIAL_OUTPUT_PATH", &target);
    let session = ViewerSession::named("undecorated-test");
    let borrowed = dbg_in!(&session; &value);
    assert_eq!(borrowed.name, "Ada");
    let session_html =
        fs::read_to_string(&target).expect("dbg_in! should capture an undecorated value");
    let captures = embedded_captures(&session_html);
    assert_eq!(
        capture_datum(&captures[0]).atoms[0].r#type,
        "UndecoratedUser"
    );
    diagram(&value);
    let read_result = fs::read_to_string(&target);
    env::remove_var("SPYTIAL_OUTPUT_PATH");
    drop(_guard);

    let contents = read_result.expect("diagram() should render an undecorated Serialize value");
    let datum = embedded_datum(&contents);
    assert_eq!(datum.atoms[0].r#type, "UndecoratedUser");
    assert!(datum
        .atoms
        .iter()
        .any(|atom| atom.r#type == "string" && atom.label == "Ada"));
    assert!(datum
        .relations
        .iter()
        .any(|relation| relation.name == "name" && relation.tuples.len() == 1));
    let _ = fs::remove_file(&target);
}

#[test]
fn diagram_applies_registered_root_and_nested_decorators() {
    suppress_browser_open();

    // Keep the child absent: transitive collection must come from the type
    // registration rather than happening accidentally while serializing a
    // child value.
    let value = RegisteredParent {
        parent_registration_marker: "present".to_string(),
        child: None,
    };
    let target = unique_output_path("registered-decorators");

    let _guard = diagram_lock();
    env::set_var("SPYTIAL_OUTPUT_PATH", &target);
    diagram(&value);
    let read_result = fs::read_to_string(&target);
    env::remove_var("SPYTIAL_OUTPUT_PATH");
    drop(_guard);

    let contents = read_result.expect("diagram() should render a decorated value");
    assert!(
        contents.contains("ROOT_REGISTRATION_SELECTOR"),
        "the root type's registered decorator should be embedded"
    );
    assert!(
        contents.contains("nested_registration_marker"),
        "the absent nested type's decorator should be collected transitively"
    );
    let _ = fs::remove_file(&target);
}

#[test]
fn anonymous_serde_roots_keep_their_decorators() {
    suppress_browser_open();

    let transparent = TransparentRoot("visible value".to_string());
    let untagged = UntaggedRoot::Text("visible variant".to_string());
    let target = unique_output_path("anonymous-serde-roots");

    let _guard = diagram_lock();
    env::set_var("SPYTIAL_OUTPUT_PATH", &target);
    let session = ViewerSession::named("anonymous-serde-test");

    let returned = dbg_in!(&session; &transparent);
    assert_eq!(returned.0, "visible value");
    let transparent_html =
        fs::read_to_string(&target).expect("dbg_in! should render a transparent root");
    let captures = embedded_captures(&transparent_html);
    assert!(captures[0]["spytial_spec"]
        .as_str()
        .expect("capture should contain a Spytial spec")
        .contains("TRANSPARENT_ROOT_SELECTOR"));

    diagram(&untagged);
    let untagged_html =
        fs::read_to_string(&target).expect("diagram should render an untagged root");
    assert!(untagged_html.contains("UNTAGGED_ROOT_SELECTOR"));

    env::remove_var("SPYTIAL_OUTPUT_PATH");
    drop(_guard);
    let _ = fs::remove_file(&target);
}

#[test]
fn same_short_type_names_do_not_mix_decorators() {
    suppress_browser_open();

    let left = collision_left::Item {
        value: "left".to_string(),
    };
    // Referencing the second type keeps both inventory entries in this test
    // binary; only the left value is diagrammed.
    let right = collision_right::Item {
        value: "right".to_string(),
    };
    assert_eq!(right.value, "right");
    // Exercise the old runtime registry too: its short `Item` key now points
    // at the right-hand type, but exact root discovery must still select left.
    let _ = collision_right::Item::decorators();
    let target = unique_output_path("same-short-type-name");

    let _guard = diagram_lock();
    env::set_var("SPYTIAL_OUTPUT_PATH", &target);
    diagram(&left);
    let read_result = fs::read_to_string(&target);
    env::remove_var("SPYTIAL_OUTPUT_PATH");
    drop(_guard);

    let contents = read_result.expect("diagram should render the selected Item type");
    assert!(contents.contains("LEFT_ITEM_SELECTOR"));
    assert!(!contents.contains("RIGHT_ITEM_SELECTOR"));
    let _ = fs::remove_file(&target);
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
    let source_file_name = Path::new(file!())
        .file_name()
        .expect("source filename")
        .to_string_lossy();
    assert!(
        contents.contains(source_file_name.as_ref()),
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
    #[hide_atom(selector = "LOCAL_DERIVE_REGISTRATION_SELECTOR")]
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
    assert!(
        contents.contains("LOCAL_DERIVE_REGISTRATION_SELECTOR"),
        "a derive on a function-local type should still register decorators",
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
