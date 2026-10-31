//! HTTP JSON bridge for the browser UI in development and for Playwright tests:
//! `POST /rpc {"method": …, "params": …}`. Same `Session::call` as the Tauri shell.

use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use crate::error::ErrorBody;
use crate::methods::Session;

fn cors(
    r: tiny_http::Response<std::io::Cursor<Vec<u8>>>,
) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    let h = |k: &str, v: &str| tiny_http::Header::from_bytes(k.as_bytes(), v.as_bytes()).ok();
    let mut r = r;
    for (k, v) in [
        ("Access-Control-Allow-Origin", "*"),
        ("Access-Control-Allow-Headers", "content-type"),
        ("Access-Control-Allow-Methods", "POST, GET, OPTIONS"),
        ("Content-Type", "application/json"),
    ] {
        if let Some(h) = h(k, v) {
            r.add_header(h);
        }
    }
    r
}

/// Handles one RPC body; returns the JSON reply.
pub fn handle(session: &Mutex<Session>, body: &str) -> Value {
    let req: Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => {
            return json!({ "ok": false, "error": { "code": "bad_request", "message": e.to_string() } })
        }
    };
    let method = req
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let params = req.get("params").cloned().unwrap_or(Value::Null);
    let Ok(mut s) = session.lock() else {
        return json!({ "ok": false, "error": { "code": "error", "message": "session lock poisoned" } });
    };
    match s.call(&method, &params) {
        Ok(v) => json!({ "ok": true, "result": v }),
        Err(e) => json!({ "ok": false, "error": ErrorBody::from(&e) }),
    }
}

/// Serves until the process ends.
pub fn serve(addr: &str, session: Session) -> crate::Result<()> {
    let server = tiny_http::Server::http(addr)
        .map_err(|e| crate::AppError::Other(format!("cannot listen on {addr}: {e}")))?;
    let session = Arc::new(Mutex::new(session));
    eprintln!(
        "{} bridge listening on http://{addr}",
        apc_core::brand::DISPLAY_NAME
    );
    for mut req in server.incoming_requests() {
        let reply = match (req.method(), req.url()) {
            (tiny_http::Method::Options, _) => tiny_http::Response::from_data(Vec::new()),
            (tiny_http::Method::Get, "/health") => {
                tiny_http::Response::from_data(b"{\"ok\":true}".to_vec())
            }
            (tiny_http::Method::Post, "/rpc") => {
                let mut body = String::new();
                let _ = req.as_reader().read_to_string(&mut body);
                let v = handle(&session, &body);
                tiny_http::Response::from_data(serde_json::to_vec(&v).unwrap_or_default())
            }
            _ => tiny_http::Response::from_data(b"{\"ok\":false}".to_vec()).with_status_code(404),
        };
        let _ = req.respond(cors(reply));
    }
    Ok(())
}
