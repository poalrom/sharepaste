use std::io::Write;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const DEFAULT_SERVER_URL: &str = "http://127.0.0.1:8443";
const SERVER_CONTAINER: &str = "sharepaste";

static NONCE: AtomicU64 = AtomicU64::new(0);

pub struct TestServer {
    pub url: String,
}

/// Returns `None` — after printing a skip notice — when no server answers
/// `/healthz`. These tests need a live stack, so a missing one is a skip, not
/// a failure: plain `cargo test` must stay green on a clean machine.
pub fn start() -> Option<TestServer> {
    let url = std::env::var("SHAREPASTE_TEST_SERVER")
        .unwrap_or_else(|_| DEFAULT_SERVER_URL.to_string());

    // Run the health check on a dedicated thread so we don't construct a
    // tokio runtime inside the outer #[tokio::test] async context.
    let probe_url = url.clone();
    let handle = std::thread::spawn(move || -> Result<(), String> {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(3))
            .build()
            .map_err(|e| e.to_string())?;
        let resp = client
            .get(format!("{probe_url}/healthz"))
            .send()
            .map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("/healthz returned {}", resp.status()));
        }
        Ok(())
    });

    match handle.join().expect("healthz thread panicked") {
        Ok(()) => Some(TestServer { url }),
        Err(why) => {
            // Written straight to the process stderr rather than via
            // `eprintln!`, which libtest captures and then discards for a
            // passing test — the notice would never reach the contributor it
            // exists to inform.
            let _ = std::io::stderr().write_all(
                format!(
                    "SKIP: no server at {url} ({why}); \
                     set SHAREPASTE_TEST_SERVER or run docker compose up -d\n"
                )
                .as_bytes(),
            );
            None
        }
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

/// Builds the operator-CLI invocation.
///
/// Two shapes, because the crate only compiles on macOS and Windows while the
/// server image only runs on Linux — so CI cannot use Docker and a local dev
/// box usually does:
///
/// * `SHAREPASTE_TEST_CLI` set (e.g. `node /path/to/server/dist/index.js`) —
///   run it directly. Used by CI, where the server is a plain node process on
///   the same runner, and by anyone without Docker.
/// * unset — `docker exec` into the running container.
///
/// Either way the CLI must resolve the *same* database the server opened. In
/// the container that means leaving `--db` off so it reads the image's own
/// `DB_PATH`; natively it means `SHAREPASTE_TEST_DB` pointing at the same file
/// the server was given. Getting this wrong lands the invite in a database the
/// server never reads, which surfaces as a bogus "invite not found".
fn cli_command() -> Command {
    if let Ok(spec) = std::env::var("SHAREPASTE_TEST_CLI") {
        let mut parts = spec.split_whitespace();
        let program = parts.next().expect("SHAREPASTE_TEST_CLI must not be empty");
        let mut cmd = Command::new(program);
        cmd.args(parts);
        return cmd;
    }

    // The CLI must run *inside* the container so it shares better-sqlite3's WAL
    // state with the running server — running from the host raced and produced
    // FK / "invite not found" errors.
    let mut cmd = Command::new("docker");
    cmd.args([
        "exec",
        SERVER_CONTAINER,
        "node",
        // The image has no `sharepaste` on PATH: package.json declares a `bin`
        // entry, but the Dockerfile never links it, and `docker exec` bypasses
        // the ENTRYPOINT that would otherwise supply `node dist/index.js`.
        "/app/dist/index.js",
    ]);
    cmd
}

pub fn create_invite(_server: &TestServer, prefix: &str) -> (String, String) {
    let username = unique_username(prefix);
    let mut cmd = cli_command();
    cmd.args(["user", "create"]);
    if let Ok(db) = std::env::var("SHAREPASTE_TEST_DB") {
        cmd.arg("--db").arg(db);
    }
    let out = cmd
        .arg(&username)
        .output()
        .expect("spawn the sharepaste operator CLI");
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
