//! Persistent viewer sessions used by [`crate::dbg!`] and [`crate::dbg_in!`].

use crate::export::export_json_instance_and_decorators;
use crate::spytial_annotations::to_yaml;
use serde::Serialize;
use serde_json::{json, Value};
use std::env;
use std::fs;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{self, Command};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::{Duration, SystemTime};

static SESSION_COUNTER: AtomicU64 = AtomicU64::new(0);
static SNAPSHOT_COUNTER: AtomicU64 = AtomicU64::new(0);

/// A persistent collection of ordered Spytial captures.
///
/// [`crate::dbg!`] uses one process-wide default session. Create a named
/// session when separate capture streams are useful, then pass it to
/// [`crate::dbg_in!`]:
///
/// ```no_run
/// use serde::Serialize;
/// use spytial::{dbg_in, SpytialDecorators, ViewerSession};
///
/// #[derive(Debug, Serialize, SpytialDecorators)]
/// struct State {
///     step: usize,
/// }
///
/// let parser = ViewerSession::named("parser");
/// let state = dbg_in!(&parser; State { step: 1 });
/// let _state = dbg_in!(&parser; state);
/// ```
///
/// A session starts lazily on its first capture. Its loopback server lives only
/// as long as the program, while its self-contained HTML snapshot remains at
/// [`ViewerSession::output_path`] for later inspection.
#[derive(Clone)]
pub struct ViewerSession {
    inner: Arc<SessionInner>,
}

struct SessionInner {
    id: String,
    name: Option<String>,
    output_path: PathBuf,
    captures: Arc<Mutex<Vec<Value>>>,
    capture_endpoint: OnceLock<String>,
    viewer: Mutex<ViewerState>,
}

#[derive(Debug, PartialEq, Eq)]
enum ViewerState {
    NotStarted,
    Started,
    SnapshotOnly,
}

impl Default for ViewerSession {
    fn default() -> Self {
        Self::new()
    }
}

impl ViewerSession {
    /// Create an unnamed viewer session.
    ///
    /// `SPYTIAL_OUTPUT_PATH`, when present, is read now and used as this
    /// session's stable HTML snapshot path. Otherwise Spytial chooses a unique
    /// file in the operating system's temporary directory.
    pub fn new() -> Self {
        Self::create(None)
    }

    /// Create a viewer session with a human-readable name.
    ///
    /// The name is displayed in the viewer. It does not replace the unique
    /// session id and does not affect the output filename.
    pub fn named(name: impl Into<String>) -> Self {
        Self::create(Some(name.into()))
    }

    fn create(name: Option<String>) -> Self {
        let id = new_session_id();
        let output_path = env::var_os("SPYTIAL_OUTPUT_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|| env::temp_dir().join(format!("spytial-session-{id}.html")));

        Self {
            inner: Arc::new(SessionInner {
                id,
                name,
                output_path,
                captures: Arc::new(Mutex::new(Vec::new())),
                capture_endpoint: OnceLock::new(),
                viewer: Mutex::new(ViewerState::NotStarted),
            }),
        }
    }

    /// Return this session's unique id.
    pub fn id(&self) -> &str {
        &self.inner.id
    }

    /// Return this session's optional display name.
    pub fn name(&self) -> Option<&str> {
        self.inner.name.as_deref()
    }

    /// Return the self-contained HTML snapshot path used by this session.
    ///
    /// The same file is atomically refreshed after every successful capture.
    /// It can be opened after the program (and its live loopback server) exits.
    pub fn output_path(&self) -> &Path {
        &self.inner.output_path
    }

    /// Add a value to this session without writing a `Debug` representation to
    /// stderr.
    ///
    /// The capture's expression label is `diagram`, and its source location is
    /// the caller of this method. For strict [`std::dbg!`] behavior and the
    /// original expression text, use [`crate::dbg_in!`]. Like every viewer
    /// operation, this method is best-effort and never panics on transport or
    /// output failure. Any [`Serialize`] value works; deriving
    /// [`crate::SpytialDecorators`] optionally enriches its layout.
    #[track_caller]
    pub fn diagram<T: Serialize>(&self, value: &T) {
        let caller = std::panic::Location::caller();
        self.capture(
            value,
            "diagram",
            caller.file(),
            caller.line(),
            caller.column(),
        );
    }

    pub(crate) fn capture<T: Serialize>(
        &self,
        value: &T,
        expression: &str,
        file: &str,
        line: u32,
        column: u32,
    ) {
        let (datum, decorators) = export_json_instance_and_decorators(value);
        let spec = to_yaml(&decorators).unwrap_or_default();
        let thread = std::thread::current();
        let timestamp_unix_ms = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or(0);

        let capture = json!({
            "session_id": self.inner.id,
            "session_name": self.inner.name,
            "sequence": 0,
            "source": {
                "file": file,
                "line": line,
                "column": column,
            },
            "expression": expression,
            "timestamp_unix_ms": timestamp_unix_ms,
            "thread": {
                "id": format!("{:?}", thread.id()),
                "name": thread.name(),
            },
            "datum": datum,
            "spytial_spec": spec,
        });

        self.append(capture);
    }

    fn append(&self, mut capture: Value) {
        let write_result = {
            let mut captures = lock_or_recover(&self.inner.captures);
            let sequence = captures.len() as u64 + 1;
            capture["sequence"] = Value::from(sequence);
            captures.push(capture);
            let endpoint = self.inner.capture_endpoint.get().map(String::as_str);
            let html = render_session_html(&captures, endpoint);
            write_snapshot(&self.inner.output_path, html.as_bytes())
        };

        if let Err(error) = &write_result {
            eprintln!(
                "spytial: could not write viewer session to {}: {error}",
                self.inner.output_path.display()
            );
        }

        if no_open() {
            if write_result.is_ok() {
                eprintln!(
                    "spytial: capture appended to {}",
                    self.inner.output_path.display()
                );
            }
            return;
        }

        self.ensure_viewer_started();
    }

    fn ensure_viewer_started(&self) {
        let captures = Arc::clone(&self.inner.captures);
        let session_id = self.inner.id.clone();
        self.ensure_viewer_started_with(
            move || start_server(captures, session_id),
            |url| {
                let target = if let Some(url) = url {
                    let snapshot_result = {
                        let captures = lock_or_recover(&self.inner.captures);
                        let endpoint = self.inner.capture_endpoint.get().map(String::as_str);
                        let html = render_session_html(&captures, endpoint);
                        write_snapshot(&self.inner.output_path, html.as_bytes())
                    };
                    if let Err(error) = &snapshot_result {
                        eprintln!(
                            "spytial: could not add live updates to {}: {error}",
                            self.inner.output_path.display()
                        );
                    }
                    if snapshot_result.is_ok() {
                        self.inner.output_path.to_string_lossy().into_owned()
                    } else {
                        url.to_string()
                    }
                } else {
                    self.inner.output_path.to_string_lossy().into_owned()
                };
                open_browser(&target)
            },
        );
    }

    fn ensure_viewer_started_with<S, O>(&self, start_server: S, open_browser: O)
    where
        S: FnOnce() -> io::Result<String>,
        O: FnOnce(Option<&str>) -> io::Result<()>,
    {
        let mut state = lock_or_recover(&self.inner.viewer);
        if *state != ViewerState::NotStarted {
            return;
        }

        let url = match start_server() {
            Ok(url) => url,
            Err(error) => {
                *state = ViewerState::SnapshotOnly;
                eprintln!(
                    "spytial: could not start the local viewer server: {error}. Falling back to {}",
                    self.inner.output_path.display()
                );
                if let Err(open_error) = open_browser(None) {
                    eprintln!(
                        "spytial: failed to open the static viewer ({open_error}). Open {} manually",
                        self.inner.output_path.display()
                    );
                }
                return;
            }
        };

        // Starting succeeded, so do not retry (and create tab spam) even when
        // the platform browser command itself fails.
        let _ = self
            .inner
            .capture_endpoint
            .set(format!("{url}sessions/{}/captures", self.inner.id));
        *state = ViewerState::Started;
        if let Err(error) = open_browser(Some(&url)) {
            eprintln!(
                "spytial: failed to open browser ({error}). Open {url} or {} manually",
                self.inner.output_path.display()
            );
        }
    }
}

pub(crate) fn default_session() -> &'static ViewerSession {
    static DEFAULT_SESSION: OnceLock<ViewerSession> = OnceLock::new();
    DEFAULT_SESSION.get_or_init(ViewerSession::new)
}

fn new_session_id() -> String {
    let counter = SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("{}-{counter}-{nanos}", process::id())
}

fn lock_or_recover<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn no_open() -> bool {
    env::var("SPYTIAL_NO_OPEN")
        .map(|raw| matches!(raw.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

fn render_session_html(captures: &[Value], capture_endpoint: Option<&str>) -> String {
    let captures_json = serde_json::to_string(captures).unwrap_or_else(|_| "[]".to_string());
    let captures_literal = safe_javascript_string(&captures_json);
    let endpoint_literal = capture_endpoint
        .map(safe_javascript_string)
        .unwrap_or_else(|| "null".to_string());

    include_str!("../templates/session.html")
        .replace(
            "/*__SPYTIAL_CORE_CSS__*/",
            include_str!("../templates/vendor/spytial-core.css"),
        )
        .replace(
            "/*__REACT_COMPONENTS_CSS__*/",
            include_str!("../templates/vendor/react-component-integration.css"),
        )
        .replace(
            "/*__SPYTIAL_CORE_JS__*/",
            include_str!("../templates/vendor/spytial-core.global.js"),
        )
        .replace(
            "/*__REACT_COMPONENTS_JS__*/",
            include_str!("../templates/vendor/react-component-integration.global.js"),
        )
        .replace("{{ captures_json_literal }}", &captures_literal)
        .replace("{{ capture_endpoint_literal }}", &endpoint_literal)
}

fn safe_javascript_string(value: &str) -> String {
    serde_json::to_string(value)
        .unwrap_or_else(|_| "\"\"".to_string())
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026")
}

fn write_snapshot(path: &Path, contents: &[u8]) -> io::Result<()> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("spytial-session.html");
    let counter = SNAPSHOT_COUNTER.fetch_add(1, Ordering::Relaxed);
    let temporary = path.with_file_name(format!(".{file_name}.tmp-{}-{counter}", process::id()));

    fs::write(&temporary, contents)?;
    match fs::rename(&temporary, path) {
        Ok(()) => Ok(()),
        Err(_) => {
            // Windows cannot atomically replace an existing destination. The
            // direct-write fallback retains the old cross-platform behavior.
            let result = fs::write(path, contents);
            let _ = fs::remove_file(temporary);
            result
        }
    }
}

fn start_server(captures: Arc<Mutex<Vec<Value>>>, session_id: String) -> io::Result<String> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let address = listener.local_addr()?;
    let capture_path = format!("/sessions/{session_id}/captures");
    std::thread::Builder::new()
        .name("spytial-viewer".to_string())
        .spawn(move || {
            for stream in listener.incoming() {
                match stream {
                    Ok(mut stream) => handle_connection(&mut stream, &captures, &capture_path),
                    Err(error) => {
                        eprintln!("spytial: local viewer connection failed: {error}");
                    }
                }
            }
        })?;
    Ok(format!("http://{address}/"))
}

fn handle_connection(stream: &mut TcpStream, captures: &Mutex<Vec<Value>>, capture_path: &str) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let mut buffer = [0_u8; 8192];
    let bytes_read = match stream.read(&mut buffer) {
        Ok(0) => return,
        Ok(bytes_read) => bytes_read,
        Err(error) => {
            eprintln!("spytial: could not read local viewer request: {error}");
            return;
        }
    };

    let request = String::from_utf8_lossy(&buffer[..bytes_read]);
    let mut request_line = request
        .lines()
        .next()
        .unwrap_or_default()
        .split_whitespace();
    let method = request_line.next().unwrap_or_default();
    let raw_path = request_line.next().unwrap_or("/");
    let path = raw_path.split('?').next().unwrap_or(raw_path);

    if method != "GET" {
        write_response(
            stream,
            "405 Method Not Allowed",
            "text/plain; charset=utf-8",
            b"Method Not Allowed",
        );
        return;
    }

    match path {
        "/" | "/index.html" => {
            let snapshot = lock_or_recover(captures);
            let html = render_session_html(&snapshot, Some(capture_path));
            write_response(
                stream,
                "200 OK",
                "text/html; charset=utf-8",
                html.as_bytes(),
            );
        }
        path if path == capture_path => {
            let snapshot = lock_or_recover(captures);
            let body = serde_json::to_vec(&*snapshot).unwrap_or_else(|_| b"[]".to_vec());
            write_response(stream, "200 OK", "application/json; charset=utf-8", &body);
        }
        "/health" => write_response(stream, "200 OK", "text/plain; charset=utf-8", b"ok"),
        "/favicon.ico" => write_response(stream, "204 No Content", "image/x-icon", b""),
        _ => write_response(
            stream,
            "404 Not Found",
            "text/plain; charset=utf-8",
            b"Not Found",
        ),
    }
}

fn write_response(stream: &mut TcpStream, status: &str, content_type: &str, body: &[u8]) {
    let headers = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nAccess-Control-Allow-Origin: null\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n",
        body.len()
    );
    if stream.write_all(headers.as_bytes()).is_ok() {
        let _ = stream.write_all(body);
    }
}

/// The platform's "open this in the default browser" command, ready to spawn.
///
/// Shared by the session viewer and the standalone [`crate::diagram`] path,
/// so there is one place that knows the platform quirks. The one that bit:
/// on Windows the opener is `start`, which is a `cmd.exe` builtin rather than
/// a program, so it has to be reached through `cmd /C`. The empty argument
/// after `start` is the window title; without it `start` takes the first
/// quoted argument (a path with a space in it) as the title and opens nothing.
///
/// Building the command is separate from spawning it so the shape can be
/// tested without opening a browser.
pub(crate) fn browser_command(target: &str) -> io::Result<Command> {
    #[cfg(target_os = "macos")]
    {
        let mut command = Command::new("open");
        command.arg(target);
        Ok(command)
    }
    #[cfg(target_os = "windows")]
    {
        let mut command = Command::new("cmd");
        command.args(["/C", "start", ""]).arg(target);
        Ok(command)
    }
    #[cfg(any(
        target_os = "linux",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "netbsd",
        target_os = "dragonfly"
    ))]
    {
        let mut command = Command::new("xdg-open");
        command.arg(target);
        Ok(command)
    }
    #[cfg(not(any(
        target_os = "macos",
        target_os = "windows",
        target_os = "linux",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "netbsd",
        target_os = "dragonfly"
    )))]
    {
        let _ = target;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "no known browser-open command for this platform",
        ))
    }
}

/// Open `target` (a file path or URL) in the default browser, without waiting.
pub(crate) fn open_browser(target: &str) -> io::Result<()> {
    browser_command(target)?.spawn().map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spytial_annotations::{HasSpytialDecorators, SpytialDecorators};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Barrier;

    fn test_session(path: PathBuf) -> ViewerSession {
        ViewerSession {
            inner: Arc::new(SessionInner {
                id: "test-session".to_string(),
                name: Some("tests".to_string()),
                output_path: path,
                captures: Arc::new(Mutex::new(Vec::new())),
                capture_endpoint: OnceLock::new(),
                viewer: Mutex::new(ViewerState::NotStarted),
            }),
        }
    }

    fn test_capture(expression: &str) -> Value {
        json!({
            "session_id": "test-session",
            "session_name": "tests",
            "sequence": 0,
            "source": { "file": "tests.rs", "line": 10, "column": 4 },
            "expression": expression,
            "timestamp_unix_ms": 1,
            "thread": { "id": "ThreadId(1)", "name": "tests" },
            "datum": { "atoms": [], "relations": [] },
            "spytial_spec": "",
        })
    }

    #[test]
    fn captures_are_appended_in_sequence_order() {
        env::set_var("SPYTIAL_NO_OPEN", "1");
        let path = env::temp_dir().join(format!("spytial-order-{}.html", new_session_id()));
        let session = test_session(path.clone());

        session.append(test_capture("first"));
        session.append(test_capture("second"));
        session.append(test_capture("third"));

        let captures = lock_or_recover(&session.inner.captures);
        let sequences: Vec<_> = captures
            .iter()
            .map(|capture| capture["sequence"].as_u64().unwrap())
            .collect();
        assert_eq!(sequences, vec![1, 2, 3]);
        assert!(fs::read_to_string(&path).unwrap().contains("third"));
        assert_eq!(
            *lock_or_recover(&session.inner.viewer),
            ViewerState::NotStarted
        );
        assert!(session.inner.capture_endpoint.get().is_none());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn concurrent_captures_are_not_lost_or_corrupted() {
        env::set_var("SPYTIAL_NO_OPEN", "1");
        let path = env::temp_dir().join(format!("spytial-concurrent-{}.html", new_session_id()));
        let session = test_session(path.clone());
        let barrier = Arc::new(Barrier::new(9));
        let mut threads = Vec::new();

        for index in 0..8 {
            let session = session.clone();
            let barrier = Arc::clone(&barrier);
            threads.push(std::thread::spawn(move || {
                barrier.wait();
                session.append(test_capture(&format!("capture-{index}")));
            }));
        }
        barrier.wait();
        for thread in threads {
            thread.join().unwrap();
        }

        let captures = lock_or_recover(&session.inner.captures);
        assert_eq!(captures.len(), 8);
        for (index, capture) in captures.iter().enumerate() {
            assert_eq!(capture["sequence"].as_u64(), Some(index as u64 + 1));
        }
        drop(captures);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn multi_argument_dbg_uses_one_session_and_preserves_metadata() {
        env::set_var("SPYTIAL_NO_OPEN", "1");

        #[derive(Debug, Serialize)]
        struct W(u32);

        impl HasSpytialDecorators for W {
            fn decorators() -> SpytialDecorators {
                SpytialDecorators::default()
            }
        }

        let path = env::temp_dir().join(format!("spytial-multi-{}.html", new_session_id()));
        let session = test_session(path.clone());
        let (first, second) = crate::dbg_in!(&session; W(1), W(2));
        assert_eq!((first.0, second.0), (1, 2));

        let captures = lock_or_recover(&session.inner.captures);
        assert_eq!(captures.len(), 2);
        assert_eq!(captures[0]["sequence"], 1);
        assert_eq!(captures[1]["sequence"], 2);
        assert_eq!(captures[0]["expression"], "W(1)");
        assert_eq!(captures[1]["expression"], "W(2)");
        assert!(captures[0]["source"]["file"]
            .as_str()
            .unwrap()
            .ends_with("session.rs"));
        assert!(captures[0]["source"]["line"].as_u64().unwrap() > 0);
        assert!(captures[0]["source"]["column"].as_u64().unwrap() > 0);
        assert!(captures[0]["thread"]["id"].as_str().is_some());
        drop(captures);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn viewer_server_and_browser_are_started_only_once() {
        let path = env::temp_dir().join(format!("spytial-open-once-{}.html", new_session_id()));
        let session = test_session(path);
        let starts = AtomicUsize::new(0);
        let opens = AtomicUsize::new(0);

        session.ensure_viewer_started_with(
            || {
                starts.fetch_add(1, Ordering::SeqCst);
                Ok("http://127.0.0.1:1234/".to_string())
            },
            |url| {
                assert_eq!(url, Some("http://127.0.0.1:1234/"));
                opens.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
        );
        session.ensure_viewer_started_with(
            || {
                starts.fetch_add(1, Ordering::SeqCst);
                Ok("http://127.0.0.1:5678/".to_string())
            },
            |_| {
                opens.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
        );
        session.ensure_viewer_started_with(
            || {
                starts.fetch_add(1, Ordering::SeqCst);
                Ok("http://127.0.0.1:9012/".to_string())
            },
            |_| {
                opens.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
        );

        assert_eq!(starts.load(Ordering::SeqCst), 1);
        assert_eq!(opens.load(Ordering::SeqCst), 1);
        assert_eq!(
            session.inner.capture_endpoint.get().map(String::as_str),
            Some("http://127.0.0.1:1234/sessions/test-session/captures")
        );
    }

    #[test]
    fn live_transport_uses_loopback_and_serves_captures() {
        let captures = Arc::new(Mutex::new(vec![test_capture("served")]));
        let url = match start_server(captures, "test-session".to_string()) {
            Ok(url) => url,
            Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
                eprintln!("skipping loopback transport test: {error}");
                return;
            }
            Err(error) => panic!("loopback viewer should start: {error}"),
        };
        assert!(url.starts_with("http://127.0.0.1:"));

        let address = url.strip_prefix("http://").unwrap().trim_end_matches('/');
        let mut stream = TcpStream::connect(address).unwrap();
        stream
            .write_all(b"GET /sessions/test-session/captures HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();

        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert!(response.contains("Access-Control-Allow-Origin: null"));
        assert!(response.contains("served"));

        let mut wrong_session = TcpStream::connect(address).unwrap();
        wrong_session
            .write_all(b"GET /sessions/other-session/captures HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .unwrap();
        let mut wrong_response = String::new();
        wrong_session.read_to_string(&mut wrong_response).unwrap();
        assert!(wrong_response.starts_with("HTTP/1.1 404 Not Found"));
    }

    #[test]
    fn transport_failure_is_best_effort_and_not_retried() {
        let path = env::temp_dir().join(format!("spytial-server-fail-{}.html", new_session_id()));
        let session = test_session(path);
        let starts = AtomicUsize::new(0);
        let opens = AtomicUsize::new(0);

        for _ in 0..2 {
            session.ensure_viewer_started_with(
                || {
                    starts.fetch_add(1, Ordering::SeqCst);
                    Err(io::Error::new(
                        io::ErrorKind::AddrNotAvailable,
                        "injected bind failure",
                    ))
                },
                |url| {
                    assert!(url.is_none());
                    opens.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                },
            );
        }

        assert_eq!(
            *lock_or_recover(&session.inner.viewer),
            ViewerState::SnapshotOnly
        );
        assert_eq!(starts.load(Ordering::SeqCst), 1);
        assert_eq!(opens.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn output_failure_is_graceful_and_capture_is_retained() {
        env::set_var("SPYTIAL_NO_OPEN", "1");
        let blocker = env::temp_dir().join(format!("spytial-not-a-dir-{}", new_session_id()));
        fs::write(&blocker, b"not a directory").unwrap();
        let session = test_session(blocker.join("viewer.html"));

        #[derive(Debug, Serialize)]
        struct W(u32);

        impl HasSpytialDecorators for W {
            fn decorators() -> SpytialDecorators {
                SpytialDecorators::default()
            }
        }

        let returned = crate::dbg_in!(&session; W(42));
        assert_eq!(returned.0, 42);

        let captures = lock_or_recover(&session.inner.captures);
        assert_eq!(captures.len(), 1);
        assert_eq!(captures[0]["expression"], "W(42)");
        drop(captures);
        let _ = fs::remove_file(blocker);
    }

    /// The launcher has to name a program the process spawner can find.
    /// `start` is a `cmd.exe` builtin, not an executable, so
    /// `Command::new("start")` fails with "file not found" on every Windows
    /// machine — which is how `diagram()` shipped, while this module had the
    /// right form. Pinning the shape here covers both callers.
    #[test]
    fn browser_command_names_a_real_executable_on_this_platform() {
        let target = "C:\\spytial dir\\session.html";
        let command = match browser_command(target) {
            Ok(command) => command,
            Err(error) if error.kind() == io::ErrorKind::Unsupported => return,
            Err(error) => panic!("could not build the browser command: {error}"),
        };
        let program = command.get_program().to_string_lossy().into_owned();
        let args: Vec<String> = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();

        assert_ne!(
            program, "start",
            "`start` is a shell builtin, not a program"
        );
        if cfg!(target_os = "windows") {
            assert_eq!(program, "cmd");
            // The empty string is the window title; without it `start` reads a
            // quoted path as the title and opens nothing.
            assert_eq!(args, ["/C", "start", "", target]);
        } else if cfg!(target_os = "macos") {
            assert_eq!(program, "open");
            assert_eq!(args, [target]);
        } else {
            assert_eq!(program, "xdg-open");
            assert_eq!(args, [target]);
        }
    }

    #[test]
    fn javascript_literal_escapes_script_terminators() {
        let literal = safe_javascript_string("</script><script>alert(1)</script>");
        assert!(!literal.contains('<'));
        assert!(!literal.contains('>'));
    }
}
