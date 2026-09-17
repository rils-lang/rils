use lsp_server::Message;
use serde_json::{Value, json};
use std::{
    fs,
    io::{BufReader, Write},
    path::PathBuf,
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc::{self, Receiver},
    },
    thread,
    time::{Duration, Instant},
};

pub fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/resilience")
        .join(name)
}

pub struct Scratch(pub PathBuf);
impl Scratch {
    pub fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "rils-lsp-resilience-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub struct Client {
    child: Child,
    stdin: ChildStdin,
    messages: Receiver<Value>,
    pub notifications: Vec<Value>,
    next: i32,
}
impl Client {
    pub fn start(params: Value, sysroot: Option<&std::path::Path>) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_rils-analyzer"));
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        command.env_remove("RILS_SYSROOT");
        if let Some(root) = sysroot {
            command.env("RILS_SYSROOT", root);
        }
        let mut child = command.spawn().unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (send, messages) = mpsc::channel();
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            while let Ok(Some(message)) = Message::read(&mut reader) {
                if send.send(serde_json::to_value(message).unwrap()).is_err() {
                    break;
                }
            }
        });
        let mut client = Self {
            child,
            stdin,
            messages,
            notifications: Vec::new(),
            next: 1,
        };
        assert!(client.request("initialize", params).get("result").is_some());
        client.notify("initialized", json!({}));
        client
    }
    fn send(&mut self, value: Value) {
        let bytes = serde_json::to_vec(&value).unwrap();
        write!(self.stdin, "Content-Length: {}\r\n\r\n", bytes.len()).unwrap();
        self.stdin.write_all(&bytes).unwrap();
        self.stdin.flush().unwrap();
    }
    pub fn notify(&mut self, method: &str, params: Value) {
        self.send(json!({"jsonrpc":"2.0", "method":method, "params":params}));
    }
    pub fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next;
        self.next += 1;
        self.send(json!({"jsonrpc":"2.0", "id":id, "method":method, "params":params}));
        let deadline = Instant::now() + Duration::from_secs(45);
        loop {
            let response = self
                .messages
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .unwrap_or_else(|error| {
                    panic!(
                        "LSP {method} failed: {error}; notifications: {:?}",
                        self.notifications
                    )
                });
            if response.get("id") == Some(&json!(id)) {
                return response;
            }
            self.notifications.push(response);
        }
    }
    pub fn open_healthy(&mut self) {
        self.notify(
            "textDocument/didOpen",
            json!({"textDocument": {
                "uri":"file:///resilience/healthy.rils", "languageId":"rils", "version":1,
                "text":fs::read_to_string(fixture("healthy.rils")).unwrap()
            }}),
        );
    }
    pub fn assert_healthy(&mut self) {
        let response = self.request(
            "textDocument/documentSymbol",
            json!({"textDocument":{"uri":"file:///resilience/healthy.rils"}}),
        );
        assert!(
            response["result"]
                .as_array()
                .is_some_and(|symbols| !symbols.is_empty()),
            "{response}"
        );
    }
    pub fn shutdown(mut self) {
        assert!(
            self.request("shutdown", Value::Null)
                .get("result")
                .is_some()
        );
        // Notifications already in flight must not turn shutdown into an error.
        self.notify("textDocument/didClose", json!({}));
        assert_eq!(
            self.request("textDocument/hover", json!({}))["error"]["code"],
            -32600
        );
        self.notify("exit", Value::Null);
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            assert!(
                Instant::now() < deadline,
                "analyzer did not exit after shutdown"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }
}
impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
