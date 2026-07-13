//! Test helper: auto-starts the Go DHT Sidecar for integration tests.
//!
//! Call `common::ensure_sidecar()` to get a `&'static SidecarGuard`,
//! then use `.endpoint` to obtain the gRPC address.
//! The sidecar is compiled on demand and started on a random free port.
//! The OS reclaims the child process when the test binary exits.

use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::time::Duration;

static SIDECAR: OnceLock<SidecarGuard> = OnceLock::new();

pub fn ensure_sidecar() -> &'static SidecarGuard {
    SIDECAR.get_or_init(|| {
        SidecarGuard::start().unwrap_or_else(|e| {
            panic!("Failed to start DHT Sidecar: {e}. Is the Go toolchain installed?");
        })
    })
}

pub struct SidecarGuard {
    pub endpoint: String,
    #[allow(dead_code)]
    child: Child,
}

impl SidecarGuard {
    fn start() -> Result<Self, String> {
        // Pick a free port
        let listener = TcpListener::bind("127.0.0.1:0")
            .map_err(|e| format!("bind failed: {e}"))?;
        let grpc_port = listener.local_addr().map_err(|e| e.to_string())?.port();
        drop(listener);

        // Build Go binary if missing
        let manifest_dir =
            std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into());
        let sidecar_dir = std::path::Path::new(&manifest_dir).join("../dht");
        let sidecar_bin = sidecar_dir.join("dht-sidecar");

        if !sidecar_bin.exists() {
            eprintln!("[test] building Go DHT Sidecar...");
            let output = Command::new("go")
                .args(["build", "-o", "dht-sidecar", "."])
                .current_dir(&sidecar_dir)
                .output()
                .map_err(|e| format!("go build failed: {e}"))?;
            if !output.status.success() {
                return Err(format!(
                    "go build failed:\n{}",
                    String::from_utf8_lossy(&output.stderr)
                ));
            }
            eprintln!("[test] Go Sidecar build complete");
        }

        // Start the sidecar
        eprintln!("[test] starting DHT Sidecar on gRPC port {grpc_port}...");
        let mut child = Command::new(&sidecar_bin)
            .args(["--grpc-port", &grpc_port.to_string(), "--cmd-port", "0"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                format!(
                    "failed to start DHT Sidecar: {e} (path: {})",
                    sidecar_bin.display()
                )
            })?;

        // Wait for gRPC to become ready
        let endpoint = format!("http://127.0.0.1:{grpc_port}");
        let ready = wait_for_grpc_ready(grpc_port, Duration::from_secs(15));

        if !ready {
            let mut stderr = String::new();
            if let Some(ref mut s) = child.stderr {
                use std::io::Read;
                let _ = s.read_to_string(&mut stderr);
            }
            child.kill().ok();
            child.wait().ok();
            return Err(format!("DHT Sidecar did not become ready.\nstderr:\n{stderr}"));
        }

        eprintln!("[test] DHT Sidecar ready: {endpoint}");
        Ok(SidecarGuard { endpoint, child })
    }
}

fn wait_for_grpc_ready(port: u16, timeout: Duration) -> bool {
    let deadline = std::time::Instant::now() + timeout;
    let addr = format!("127.0.0.1:{port}");

    while std::time::Instant::now() < deadline {
        if std::net::TcpStream::connect_timeout(
            &addr.parse().unwrap(),
            Duration::from_millis(500),
        )
        .is_ok()
        {
            std::thread::sleep(Duration::from_millis(300));
            return true;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    false
}
