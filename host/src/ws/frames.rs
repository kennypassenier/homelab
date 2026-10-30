//! Reading one frame of a session: which frames carry a request, what is
//! logged for one that cannot be read, and the refusal of an unknown field.

use crate::*;

/// The journal line for a request frame this end could not parse.
///
/// fix-55 (expert panel, unparseable-frame-logged-whole, 2026-09-27): never
/// the body. A deploy frame carries every `.env` value of its stack and a
/// staging frame tens of MB of base64; both went into one journal line when
/// a version skew made a frame unreadable. serde's own message is left out
/// too, because it quotes the offending value ("invalid type: string
/// \"...\""), which can be a secret. The method, the size and where parsing
/// stopped are enough to tell which client sent what.
pub(crate) fn unparseable_frame_line(e: &serde_json::Error, text: &str) -> String {
    let method = text
        .find("\"cmd\"")
        .map(|at| {
            text[at + 5..]
                .trim_start_matches(|c: char| c.is_whitespace() || c == ':' || c == '"')
                .chars()
                .take_while(|c| c.is_ascii_lowercase() || *c == '_')
                .take(40)
                .collect::<String>()
        })
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| "?".into());
    format!(
        "unparseable request dropped :: cmd={} · {} bytes · {:?} error at line {} column {}",
        method,
        text.len(),
        e.classify(),
        e.line(),
        e.column()
    )
}

/// fix-211 (Kenny, 2026-10-02, decision "Host weigert enkel onbekende
/// velden"): why `req` should be refused before it reaches its handler, if
/// it should — a field `value` (the request exactly as the wire sent it)
/// carries that this build's own struct has no room for
/// (`homelab_core::wire::unknown_fields`), named by its path. `None` for a
/// read-only command (it changes nothing, so an unknown field in it cannot
/// cause the 2026-08-31 kind of harm) or for a request with no such field.
pub(crate) fn unknown_field_refusal(value: &serde_json::Value, req: &RpcRequest) -> Option<String> {
    if req.command.is_read_only() {
        return None;
    }
    let unknown = homelab_core::wire::unknown_fields(value, req);
    if unknown.is_empty() {
        return None;
    }
    Some(format!(
        "refused: this host's build does not know {} — update the host \
         ('homelab release-update') and try again, or drop the field if it \
         was sent by mistake",
        unknown.join(", ")
    ))
}

/// What one frame of the session turned out to be.
pub(super) enum Frame {
    /// A request, with the JSON exactly as the wire sent it (for the
    /// unknown-field check).
    Request(serde_json::Value, RpcRequest),
    /// Nothing to answer: a keepalive, a binary frame or a frame that cannot
    /// be understood (already logged).
    Skip,
    /// The session is over: the client closed or the read failed.
    End,
}

/// fix-127 (websocket-edge-cases, 2026-09-27): only Close and a read error
/// end the session. The loop matched Text alone, so the first Ping (a client
/// or proxy keepalive), Pong or Binary frame ended it without a word. Pings
/// are answered by the WebSocket layer itself.
pub(super) fn read_frame(frame: Result<Message, axum::Error>) -> Frame {
    let text = match frame {
        Ok(Message::Text(text)) => text,
        Ok(Message::Ping(_)) | Ok(Message::Pong(_)) => return Frame::Skip,
        Ok(Message::Binary(b)) => {
            tracing::warn!(
                "a binary frame of {} bytes was ignored — requests are JSON text",
                b.len()
            );
            return Frame::Skip;
        }
        Ok(Message::Close(_)) => return Frame::End,
        Err(e) => {
            info!("session ended on a read error :: {}", e);
            return Frame::End;
        }
    };
    // Silence here is why `homelab checks answer` looked like a hang rather
    // than a bug: the request was dropped and the client waited for a reply
    // that was never coming. A frame this end cannot understand is a fault
    // on this end.
    let value: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("{}", unparseable_frame_line(&e, &text));
            return Frame::Skip;
        }
    };
    match serde_json::from_value::<RpcRequest>(value.clone()) {
        Ok(req) => Frame::Request(value, req),
        Err(e) => {
            tracing::error!("{}", unparseable_frame_line(&e, &text));
            Frame::Skip
        }
    }
}
