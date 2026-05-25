use std::io::{BufRead, BufReader};
use std::net::SocketAddr;
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Runtime};

use crate::error::{Error, Result};
use crate::library::LibraryState;
use crate::opds::OpdsServer;

/// What we publish to the frontend over `share-status` events and the
/// `share_status` command.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareStatus {
    pub running: bool,
    pub local_url: Option<String>,
    pub public_url: Option<String>,
    pub error: Option<String>,
    /// "starting" | "running" | "stopped" | "error"
    pub stage: String,
}

/// Lines coming out of ngrok's stdout reader thread.
enum LogLine {
    Line(String),
    Eof,
}

struct Inner {
    opds: Option<OpdsServer>,
    ngrok: Option<Child>,
    log_thread: Option<JoinHandle<()>>,
    status: ShareStatus,
}

pub struct ShareState {
    inner: Mutex<Inner>,
}

impl ShareState {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner {
                opds: None,
                ngrok: None,
                log_thread: None,
                status: ShareStatus {
                    running: false,
                    local_url: None,
                    public_url: None,
                    error: None,
                    stage: "stopped".into(),
                },
            }),
        }
    }

    pub fn status(&self) -> ShareStatus {
        self.inner.lock().expect("share lock").status.clone()
    }

    /// Start OPDS, then ngrok. Blocks until ngrok prints a public URL (or
    /// times out after ~15 s). Idempotent: if already running, returns the
    /// existing status.
    pub fn start<R: Runtime>(
        self: &Arc<Self>,
        app: &AppHandle<R>,
        library: Arc<LibraryState>,
        domain: Option<String>,
        pooling: bool,
    ) -> Result<ShareStatus> {
        {
            let g = self.inner.lock().expect("share lock");
            if g.status.running {
                return Ok(g.status.clone());
            }
        }

        emit_status(
            app,
            &ShareStatus {
                running: false,
                local_url: None,
                public_url: None,
                error: None,
                stage: "starting".into(),
            },
        );

        let opds = OpdsServer::start(library).map_err(|e| {
            self.set_error(&format!("не смог запустить OPDS: {e}"));
            e
        })?;
        let addr = opds.addr();
        let local_url = format!("http://{addr}/opds");

        // Start ngrok
        let mut child = match spawn_ngrok(addr, domain.as_deref(), pooling) {
            Ok(c) => c,
            Err(e) => {
                opds.stop();
                self.set_error(&format!("{e}"));
                emit_status(app, &self.status());
                return Err(e);
            }
        };

        let stdout = match child.stdout.take() {
            Some(s) => s,
            None => {
                let _ = child.kill();
                let _ = child.wait();
                opds.stop();
                let e = Error::Other("ngrok: stdout не доступен".into());
                self.set_error(&format!("{e}"));
                emit_status(app, &self.status());
                return Err(e);
            }
        };
        let (tx, rx) = mpsc::channel::<LogLine>();
        let log_thread = spawn_log_reader(stdout, tx);

        let public_url = match wait_for_public_url(&mut child, &rx, Duration::from_secs(20)) {
            Ok(u) => u,
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = log_thread.join();
                opds.stop();
                self.set_error(&format!("{e}"));
                emit_status(app, &self.status());
                return Err(e);
            }
        };

        // Keep reading stdout so the pipe doesn't fill up — just log lines now.
        let log_thread = spawn_log_consumer(rx, log_thread);

        {
            let mut g = self.inner.lock().expect("share lock");
            g.opds = Some(opds);
            g.ngrok = Some(child);
            g.log_thread = Some(log_thread);
            g.status = ShareStatus {
                running: true,
                local_url: Some(local_url),
                public_url: Some(public_url),
                error: None,
                stage: "running".into(),
            };
        }
        let status = self.status();
        emit_status(app, &status);
        Ok(status)
    }

    pub fn stop<R: Runtime>(&self, app: &AppHandle<R>) -> Result<ShareStatus> {
        let (opds, mut ngrok, log_thread) = {
            let mut g = self.inner.lock().expect("share lock");
            let opds = g.opds.take();
            let ngrok = g.ngrok.take();
            let log = g.log_thread.take();
            g.status = ShareStatus {
                running: false,
                local_url: None,
                public_url: None,
                error: None,
                stage: "stopped".into(),
            };
            (opds, ngrok, log)
        };

        if let Some(c) = ngrok.as_mut() {
            let _ = c.kill();
            let _ = c.wait();
        }
        drop(ngrok);
        if let Some(t) = log_thread {
            let _ = t.join();
        }
        if let Some(o) = opds {
            o.stop();
        }
        let status = self.status();
        emit_status(app, &status);
        Ok(status)
    }

    fn set_error(&self, msg: &str) {
        let mut g = self.inner.lock().expect("share lock");
        g.status = ShareStatus {
            running: false,
            local_url: None,
            public_url: None,
            error: Some(msg.to_string()),
            stage: "error".into(),
        };
    }
}

fn spawn_ngrok(addr: SocketAddr, domain: Option<&str>, pooling: bool) -> Result<Child> {
    let port = addr.port();
    let mut cmd = Command::new("ngrok");
    cmd.args([
        "http",
        &format!("127.0.0.1:{port}"),
        "--log",
        "stdout",
        "--log-format",
        "json",
    ]);
    if let Some(d) = domain.map(str::trim).filter(|s| !s.is_empty()) {
        cmd.arg(format!("--domain={d}"));
    }
    if pooling {
        cmd.arg("--pooling-enabled");
    }
    cmd.stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => Error::Other(
                "ngrok CLI не найден в PATH. Установите ngrok (https://ngrok.com/download) и выполните `ngrok config add-authtoken <ваш токен>`."
                    .into(),
            ),
            _ => Error::Other(format!("не смог запустить ngrok: {e}")),
        })
}

/// Kill any stray `ngrok` processes locally AND clear orphan tunnels on the
/// ngrok edge via API. Necessary because ERR_NGROK_334 ("endpoint already
/// online") often comes from a session lingering on ngrok's servers — a
/// local pkill can't touch that. Returns the number of things cleared.
/// Refuses to run while our own ngrok is up — we'd take ourselves down.
pub fn kill_stray_ngrok(running: bool) -> Result<u32> {
    if running {
        return Err(Error::Other(
            "сначала остановите шаринг — иначе мы убьём собственный туннель".into(),
        ));
    }
    let local = platform_pkill_ngrok().unwrap_or(0);
    let remote = api_cleanup();
    Ok(local + remote)
}

/// Attempt to clear edge-side orphan state through `ngrok api`. Each call is
/// best-effort: if the user hasn't configured an API key, every command will
/// fail silently and we return 0.
fn api_cleanup() -> u32 {
    let mut count = 0u32;
    // Stopping the tunnel session is what actually frees the reserved
    // hostname on the free tier — endpoints are tied to their session.
    if let Some(ids) = api_list_ids(&["api", "tunnel-sessions", "list"], "tunnel_sessions") {
        for id in ids {
            if api_run(&["api", "tunnel-sessions", "stop", &id]) {
                count += 1;
            }
        }
    }
    if let Some(ids) = api_list_ids(&["api", "endpoints", "list"], "endpoints") {
        for id in ids {
            if api_run(&["api", "endpoints", "delete", &id]) {
                count += 1;
            }
        }
    }
    count
}

fn api_list_ids(args: &[&str], json_key: &str) -> Option<Vec<String>> {
    let out = Command::new("ngrok").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    let arr = v.get(json_key).and_then(|x| x.as_array())?;
    let mut out_vec = Vec::new();
    for item in arr {
        if let Some(id) = item.get("id").and_then(|x| x.as_str()) {
            out_vec.push(id.to_string());
        }
    }
    Some(out_vec)
}

fn api_run(args: &[&str]) -> bool {
    Command::new("ngrok")
        .args(args)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(unix)]
fn platform_pkill_ngrok() -> Result<u32> {
    // `pkill` exit codes: 0 = killed at least one, 1 = no match. Anything
    // else is a real failure (permission denied, etc.).
    let out = Command::new("pkill")
        .args(["-x", "ngrok"])
        .output()
        .map_err(|e| Error::Other(format!("pkill: {e}")))?;
    match out.status.code() {
        Some(0) => Ok(1),
        Some(1) => Ok(0),
        Some(n) => Err(Error::Other(format!("pkill завершился с кодом {n}"))),
        None => Err(Error::Other("pkill прерван сигналом".into())),
    }
}

#[cfg(windows)]
fn platform_pkill_ngrok() -> Result<u32> {
    let out = Command::new("taskkill")
        .args(["/F", "/IM", "ngrok.exe"])
        .output()
        .map_err(|e| Error::Other(format!("taskkill: {e}")))?;
    // taskkill exits 128 ("not found") when nothing matched.
    match out.status.code() {
        Some(0) => Ok(1),
        Some(128) => Ok(0),
        Some(n) => Err(Error::Other(format!("taskkill завершился с кодом {n}"))),
        None => Err(Error::Other("taskkill прерван".into())),
    }
}

/// Best-effort discovery of reserved domains on the user's ngrok account.
/// Returns an empty vec if API access isn't configured — the picker is
/// optional, so we never block startup on this.
pub fn list_reserved_domains() -> Vec<String> {
    let try_list = |args: &[&str]| -> Option<Vec<String>> {
        let out = Command::new("ngrok").args(args).output().ok()?;
        if !out.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&out.stdout);
        let v: serde_json::Value = serde_json::from_str(&text).ok()?;
        // Try both shapes: `{"reserved_domains":[...]}` and a top-level array.
        let arr = v
            .get("reserved_domains")
            .and_then(|x| x.as_array())
            .cloned()
            .or_else(|| v.as_array().cloned())?;
        let mut out_vec = Vec::new();
        for item in arr {
            if let Some(d) = item.get("domain").and_then(|x| x.as_str()) {
                out_vec.push(d.to_string());
            }
        }
        Some(out_vec)
    };

    try_list(&["api", "reserved-domains", "list"])
        .or_else(|| try_list(&["api", "domains", "list"]))
        .unwrap_or_default()
}

/// Read ngrok's JSON log stream until we see a line announcing the tunnel URL.
/// We also surface common error lines (auth, account limits) up to the user.
fn wait_for_public_url(
    child: &mut Child,
    rx: &mpsc::Receiver<LogLine>,
    timeout: Duration,
) -> Result<String> {
    let deadline = Instant::now() + timeout;
    let mut last_err: Option<String> = None;

    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(Error::Other(last_err.unwrap_or_else(|| {
                "ngrok не прислал публичный URL за 20 секунд".into()
            })));
        }

        // Cap individual wait so we also notice if the child has died.
        let step = remaining.min(Duration::from_millis(250));
        match rx.recv_timeout(step) {
            Ok(LogLine::Line(line)) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if let Some(url) = parse_url_line(trimmed) {
                    return Ok(url);
                }
                if let Some(err) = parse_error_line(trimmed) {
                    last_err = Some(err);
                }
            }
            Ok(LogLine::Eof) => {
                // Reader closed — surface whatever the child says.
                return Err(Error::Other(
                    last_err.unwrap_or_else(|| "ngrok закрыл stdout до публичного URL".into()),
                ));
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if let Some(status) = child.try_wait().ok().flatten() {
                    let mut err_out = String::new();
                    if let Some(mut stderr) = child.stderr.take() {
                        use std::io::Read;
                        let _ = stderr.read_to_string(&mut err_out);
                    }
                    let detail = last_err
                        .clone()
                        .or_else(|| {
                            let s = err_out.trim();
                            if s.is_empty() {
                                None
                            } else {
                                Some(s.to_string())
                            }
                        })
                        .unwrap_or_else(|| format!("ngrok завершился с {status}"));
                    return Err(Error::Other(detail));
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(Error::Other(
                    last_err.unwrap_or_else(|| "ngrok: reader disconnect".into()),
                ));
            }
        }
    }
}

fn spawn_log_reader(stdout: ChildStdout, tx: mpsc::Sender<LogLine>) -> JoinHandle<()> {
    thread::Builder::new()
        .name("ngrok-stdout".into())
        .spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(l) => {
                        if tx.send(LogLine::Line(l)).is_err() {
                            return;
                        }
                    }
                    Err(_) => {
                        let _ = tx.send(LogLine::Eof);
                        return;
                    }
                }
            }
            let _ = tx.send(LogLine::Eof);
        })
        .expect("spawn ngrok-stdout reader")
}

fn spawn_log_consumer(
    rx: mpsc::Receiver<LogLine>,
    reader_thread: JoinHandle<()>,
) -> JoinHandle<()> {
    thread::Builder::new()
        .name("ngrok-stdout-drain".into())
        .spawn(move || {
            while let Ok(line) = rx.recv() {
                match line {
                    LogLine::Line(l) => tracing::debug!(target = "ngrok", "{l}"),
                    LogLine::Eof => break,
                }
            }
            let _ = reader_thread.join();
        })
        .expect("spawn ngrok-stdout-drain")
}

fn parse_url_line(line: &str) -> Option<String> {
    // ngrok JSON log line example:
    // {"addr":"http://127.0.0.1:NNNN","lvl":"info","msg":"started tunnel",
    //  "name":"command_line","obj":"tunnels","t":"...","url":"https://abc.ngrok-free.app"}
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    let obj = v.get("obj")?.as_str()?;
    if obj != "tunnels" {
        return None;
    }
    let msg = v.get("msg").and_then(|m| m.as_str()).unwrap_or("");
    if msg != "started tunnel" {
        return None;
    }
    v.get("url")?.as_str().map(|s| s.to_string())
}

fn parse_error_line(line: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    let lvl = v.get("lvl").and_then(|x| x.as_str()).unwrap_or("");
    if lvl != "eror" && lvl != "error" && lvl != "crit" {
        return None;
    }
    let msg = v.get("msg").and_then(|x| x.as_str()).unwrap_or("ngrok error");
    let err = v.get("err").and_then(|x| x.as_str()).unwrap_or("");
    if err.is_empty() {
        Some(msg.to_string())
    } else {
        Some(format!("{msg}: {err}"))
    }
}

fn emit_status<R: Runtime>(app: &AppHandle<R>, status: &ShareStatus) {
    let _ = app.emit("share-status", status);
}
