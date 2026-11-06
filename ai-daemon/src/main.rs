//! HiTech-OS local inference & autonomic nerve daemon.

mod backend;
mod nerve;

use nerve::NervePulse;
use serde_json::{json, Value};
use std::env;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

struct DaemonState {
    backend: Box<dyn backend::InferenceBackend>,
    model_loaded: bool,
    #[allow(dead_code)]
    model_path: Option<String>,
    last_pulse: NervePulse,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("hitechos-ai-daemon v0.2.0 — started");

    let mut backend = backend::select_backend()?;
    let mut model_loaded = false;
    let mut model_path_str = None;

    if let Ok(path) = env::var("HITECHOS_MODEL_PATH") {
        if !path.trim().is_empty() {
            let pb = PathBuf::from(&path);
            if pb.exists() {
                if backend.load_model(&pb).is_ok() {
                    model_loaded = true;
                    model_path_str = Some(path);
                }
            }
        }
    }

    let initial_pulse = NervePulse::sense();
    let state = Arc::new(Mutex::new(DaemonState {
        backend,
        model_loaded,
        model_path: model_path_str,
        last_pulse: initial_pulse,
    }));

    let state_nerve = state.clone();
    thread::spawn(move || loop {
        thread::sleep(Duration::from_secs(3));
        let pulse = NervePulse::sense();
        if let Ok(mut st) = state_nerve.lock() {
            st.last_pulse = pulse;
        }
    });

    let bind_addr = env::var("HITECHOS_AI_BIND").unwrap_or_else(|_| "127.0.0.1:11435".to_string());
    let listener = TcpListener::bind(&bind_addr)?;
    println!("hitechos-ai-daemon listening on http://{bind_addr}");

    for stream in listener.incoming() {
        if let Ok(stream) = stream {
            let st = state.clone();
            thread::spawn(move || {
                let _ = handle_http(stream, st);
            });
        }
    }
    Ok(())
}

fn handle_http(mut stream: TcpStream, state: Arc<Mutex<DaemonState>>) -> std::io::Result<()> {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    if reader.read_line(&mut request_line)? == 0 {
        return Ok(());
    }
    let parts: Vec<&str> = request_line.trim().split_whitespace().collect();
    if parts.len() < 2 {
        return send_json(&mut stream, 400, &json!({"error": "bad request"}));
    }

    let (method, path) = (parts[0], parts[1]);
    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 || line == "\r\n" || line == "\n" {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            if k.trim().eq_ignore_ascii_case("content-length") {
                content_length = v.trim().parse().unwrap_or(0);
            }
        }
    }

    let mut body_bytes = vec![0u8; content_length.min(1024 * 1024)];
    if content_length > 0 {
        reader.read_exact(&mut body_bytes)?;
    }

    match (method, path) {
        ("GET", "/health") | ("GET", "/api/tags") => {
            let st = state.lock().unwrap_or_else(|p| p.into_inner());
            send_json(
                &mut stream,
                200,
                &json!({
                    "status": "ok", "service": "hitechos-ai-daemon",
                    "backend": st.backend.name(), "model_loaded": st.model_loaded,
                    "models": [{"name": "hitechos-rust-native:latest"}], "nerve": st.last_pulse
                }),
            )
        }
        ("POST", "/api/chat") => {
            let req_json: Value = serde_json::from_slice(&body_bytes).unwrap_or_else(|_| json!({}));
            let prompt = extract_prompt(&req_json);
            let max_tokens = req_json
                .get("options")
                .and_then(|o| o.get("num_predict"))
                .and_then(|n| n.as_i64())
                .map(|n| n as i32);

            let reply = {
                let mut st = state.lock().unwrap_or_else(|p| p.into_inner());
                st.backend
                    .plan_react(&prompt, max_tokens)
                    .unwrap_or_else(|_| "{\"action\":null}".to_string())
            };

            send_json(
                &mut stream,
                200,
                &json!({
                    "model": "hitechos-rust-native",
                    "message": {"role": "assistant", "content": reply},
                    "done": true
                }),
            )
        }
        _ => send_json(&mut stream, 404, &json!({"error": "not found"})),
    }
}

fn extract_prompt(req: &Value) -> String {
    let Some(msgs) = req.get("messages").and_then(|m| m.as_array()) else {
        return req
            .get("prompt")
            .and_then(|p| p.as_str())
            .unwrap_or("")
            .to_string();
    };
    msgs.iter()
        .map(|m| {
            format!(
                "[{}]\n{}\n",
                m.get("role").and_then(|r| r.as_str()).unwrap_or("user"),
                m.get("content").and_then(|c| c.as_str()).unwrap_or("")
            )
        })
        .collect()
}

fn send_json(stream: &mut TcpStream, status: u16, payload: &Value) -> std::io::Result<()> {
    let body = serde_json::to_vec(payload).unwrap_or_default();
    let header = format!("HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
    stream.write_all(header.as_bytes())?;
    stream.write_all(&body)?;
    stream.flush()
}
