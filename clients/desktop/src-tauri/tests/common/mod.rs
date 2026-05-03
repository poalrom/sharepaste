use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const SERVER_URL: &str = "http://127.0.0.1:8443";
// Path inside the `sharepaste` Docker container; the host bind-mounts
// ./db/db.sqlite to this path. The CLI must run *inside* the container so
// it shares better-sqlite3's WAL state with the running server — running
// from the host raced and produced FK / "invite not found" errors.
const SERVER_DB_IN_CONTAINER: &str = "/var/lib/sharepaste/db.sqlite";
const SERVER_CONTAINER: &str = "sharepaste";

static NONCE: AtomicU64 = AtomicU64::new(0);

pub struct TestServer {
    pub url: String,
}

pub fn start() -> TestServer {
    // Verify the running server reachable; do not spawn — user manages it.
    // Run the health check on a dedicated thread so we don't construct a
    // tokio runtime inside the outer #[tokio::test] async context.
    let handle = std::thread::spawn(|| -> Result<(), String> {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(3))
            .build()
            .map_err(|e| e.to_string())?;
        let resp = client
            .get(format!("{SERVER_URL}/healthz"))
            .send()
            .map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("/healthz returned {}", resp.status()));
        }
        Ok(())
    });
    handle
        .join()
        .expect("healthz thread panicked")
        .expect("server not reachable at 127.0.0.1:8443; start sharepaste serve first");
    TestServer {
        url: SERVER_URL.into(),
    }
}

fn unique_username(prefix: &str) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let nonce = NONCE.fetch_add(1, Ordering::SeqCst);
    format!("{prefix}-{now}-{nonce}")
}

pub fn create_invite(_server: &TestServer, prefix: &str) -> (String, String) {
    let username = unique_username(prefix);
    let out = Command::new("docker")
        .arg("exec")
        .arg(SERVER_CONTAINER)
        .arg("node")
        .arg("/app/dist/src/index.js")
        .arg("user")
        .arg("create")
        .arg("--db")
        .arg(SERVER_DB_IN_CONTAINER)
        .arg(&username)
        .output()
        .expect("spawn docker exec sharepaste user create");
    if !out.status.success() {
        panic!(
            "user create failed: stderr={}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let stdout = String::from_utf8(out.stdout).expect("user create stdout utf-8");
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("user create stdout not JSON ({e}): {stdout}"));
    let token = parsed["invite_token"]
        .as_str()
        .expect("invite_token in JSON output")
        .to_string();
    (username, token)
}
