use core::fmt;
use std::io::{self, BufRead, Write};

use log::{debug, warn};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::{bytecode, error::ExtractError};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum Message {
    Request(Request),
    Response(Response),
    Notification(Notification),
}

impl From<Request> for Message {
    fn from(value: Request) -> Self {
        Message::Request(value)
    }
}

impl From<Response> for Message {
    fn from(value: Response) -> Self {
        Message::Response(value)
    }
}

impl From<Notification> for Message {
    fn from(value: Notification) -> Self {
        Message::Notification(value)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(untagged)]
enum IdRepr {
    I32(i32),
    String(String),
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(transparent)]
pub struct RequestId(IdRepr);

impl From<i32> for RequestId {
    fn from(value: i32) -> Self {
        RequestId(IdRepr::I32(value))
    }
}

impl From<String> for RequestId {
    fn from(value: String) -> Self {
        RequestId(IdRepr::String(value))
    }
}
// Implement From<RequestId> for i32 TODO 确保都使用数字类型吧
impl From<RequestId> for i32 {
    fn from(request_id: RequestId) -> Self {
        match request_id.0 {
            IdRepr::I32(value) => value,
            IdRepr::String(_) => panic!("RequestId does not contain an i32 value"),
        }
    }
}

impl fmt::Display for RequestId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            IdRepr::I32(it) => fmt::Display::fmt(it, f),
            // Use debug here, to make it clear that `92` and `"92"` are
            // different, and to reduce WTF factor if the server users `" "` as an
            // ID.
            IdRepr::String(it) => fmt::Debug::fmt(it, f),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CompletionContext {
    pub line: String,
    pub prefix: String,
    #[serde(rename = "startPoint")]
    pub start_point: i32,
    #[serde(rename = "boundsStart")]
    pub bounds_start: i32,
    #[serde(rename = "triggerKind")]
    pub trigger_kind: lsp_types::CompletionTriggerKind,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ResolveContext {
    #[serde(rename = "language-server-id")]
    pub language_server_id: usize,
    pub start: i32,
    pub end: i32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorkspaceContext {
    #[serde(rename = "workspace-root")]
    pub workspace_root: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CommonContext {
    #[serde(rename = "language-server-id")]
    pub language_server_id: usize,
}

// #[derive(Debug, Serialize, Deserialize, Clone)]
// pub struct SignatureHelpContext {
//     #[serde(rename = "signature-trigger-character")]
//     pub signature_trigger_character: String,
// }

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum Context {
    CompletionContext(CompletionContext),
    ResolveContext(ResolveContext),
    CommonContext(CommonContext),
    WorkspaceContext(WorkspaceContext),
    // SignatureHelpContext(SignatureHelpContext),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Params {
    pub uri: Option<String>,
    pub context: Option<Context>,
    #[serde(default = "serde_json::Value::default")]
    #[serde(skip_serializing_if = "serde_json::Value::is_null")]
    pub params: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Request {
    pub id: RequestId,
    pub method: String,
    pub params: Params,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Response {
    // JSON RPC allows this to be null if it was impossible
    // to decode the request's id. Ignore this special case and
    // just die horribly
    pub id: RequestId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ResponseError>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ResponseError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Notification {
    pub method: String,
    pub params: Params,
}

impl Message {
    pub fn read(r: &mut impl BufRead) -> io::Result<Option<Message>> {
        Message::_read(r)
    }

    fn _read(r: &mut dyn BufRead) -> io::Result<Option<Message>> {
        let text = match read_msg_text(r)? {
            None => return Ok(None),
            Some(text) => text,
        };
        let msg = serde_json::from_str(&text)?;
        Ok(Some(msg))
    }

    pub fn write(self, w: &mut impl Write) -> io::Result<()> {
        self._write(w)
    }
    pub fn _write(self, w: &mut impl Write) -> io::Result<()> {
        #[derive(Serialize)]
        struct JsonRpc {
            jsonrpc: &'static str,
            #[serde(flatten)]
            msg: Message,
        }
        let json_val = serde_json::to_value(&JsonRpc {
            jsonrpc: "2.0",
            msg: self.clone(),
        })?;

        let text = serde_json::to_string(&JsonRpc {
            jsonrpc: "2.0",
            msg: self,
        })?;

        // debug!("> {}", text);

        match bytecode::generate_bytecode_repl(&json_val, bytecode::BytecodeOptions::default()) {
            Ok(bytecode_str) => {
                // debug!(
                //     "server->client: json {} byteds, converted to bytecode, {} bytes",
                //     text.len(),
                //     bytecode_str.len()
                // );
                write_msg_text(w, &bytecode_str)
            }
            Err(err) => {
                warn!("Failed to convert json to bytecode: {}", err);
                write_msg_text(w, &text)
            }
        }
    }
}

impl Response {
    pub fn new_ok<R: Serialize>(id: RequestId, result: R) -> Response {
        Response {
            id,
            result: Some(serde_json::to_value(result).unwrap()),
            error: None,
        }
    }
    pub fn new_err(id: RequestId, code: i32, message: String) -> Response {
        let error = ResponseError {
            code,
            message,
            data: None,
        };
        Response {
            id,
            result: None,
            error: Some(error),
        }
    }
}

#[allow(dead_code)]
impl Request {
    pub fn new<P: Serialize>(id: RequestId, method: String, params: P) -> Request {
        Request {
            id,
            method,
            params: Params {
                uri: None,
                context: None,
                params: serde_json::to_value(params).unwrap(),
            },
        }
    }

    pub fn uri(&self) -> Option<String> {
        self.params.uri.to_owned()
    }

    pub fn extract<P: DeserializeOwned>(
        self,
        method: &str,
    ) -> Result<(RequestId, P), ExtractError<Request>> {
        if self.method != method {
            return Err(ExtractError::MethodMismatch(self));
        }
        match serde_json::from_value(self.params.params) {
            Ok(params) => Ok((self.id, params)),
            Err(error) => Err(ExtractError::JsonError {
                method: self.method,
                error,
            }),
        }
    }

    pub(crate) fn is_shutdown(&self) -> bool {
        self.method == "shutdown"
    }
    pub(crate) fn is_initialize(&self) -> bool {
        self.method == "initialize"
    }
}

#[allow(dead_code)]
impl Notification {
    pub fn new(method: String, params: impl Serialize) -> Notification {
        Notification {
            method,
            params: Params {
                uri: None,
                context: None,
                params: serde_json::to_value(params).unwrap(),
            },
        }
    }

    pub fn uri(&self) -> Option<String> {
        self.params.uri.to_owned()
    }

    pub fn extract<P: DeserializeOwned>(
        self,
        method: &str,
    ) -> Result<P, ExtractError<Notification>> {
        if self.method != method {
            return Err(ExtractError::MethodMismatch(self));
        }
        match serde_json::from_value(self.params.params) {
            Ok(params) => Ok(params),
            Err(error) => Err(ExtractError::JsonError {
                method: self.method,
                error,
            }),
        }
    }
    pub(crate) fn is_exit(&self) -> bool {
        self.method == "exit"
    }
    pub(crate) fn is_initialize(&self) -> bool {
        self.method == "initialize"
    }
}

fn read_msg_text(inp: &mut dyn BufRead) -> io::Result<Option<String>> {
    fn invalid_data(error: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> io::Error {
        io::Error::new(io::ErrorKind::InvalidData, error)
    }
    macro_rules! invalid_data {
        ($($tt:tt)*) => (invalid_data(format!($($tt)*)))
    }

    let mut size = None;
    let mut buf = String::new();

    loop {
        buf.clear();
        if inp.read_line(&mut buf)? == 0 {
            return Ok(None);
        }
        if !buf.ends_with("\r\n") {
            return Err(invalid_data!("malformed header: {:?}", buf));
        }
        let buf = &buf[..buf.len() - 2];
        if buf.is_empty() {
            break;
        }
        let mut parts = buf.splitn(2, ": ");
        let header_name = parts.next().unwrap();
        let header_value = parts
            .next()
            .ok_or_else(|| invalid_data!("malformed header: {:?}", buf))?;
        if header_name.eq_ignore_ascii_case("Content-Length") {
            size = Some(header_value.parse::<usize>().map_err(invalid_data)?);
        }
    }

    let size: usize = size.ok_or_else(|| invalid_data!("no Content-Length"))?;
    let mut buf = buf.into_bytes();
    buf.resize(size, 0);
    inp.read_exact(&mut buf)?;
    let buf = String::from_utf8(buf).map_err(invalid_data)?;
    debug!("< {}", buf);
    Ok(Some(buf))
}

fn write_msg_text(out: &mut dyn Write, msg: &str) -> io::Result<()> {
    // debug!("> {}", msg);
    write!(out, "Content-Length: {}\r\n\r\n", msg.len())?;
    out.write_all(msg.as_bytes())?;
    out.flush()?;
    Ok(())
}

#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
#[allow(dead_code)]
pub enum ErrorCode {
    // Defined by JSON RPC:
    ParseError = -32700,
    InvalidRequest = -32600,
    MethodNotFound = -32601,
    InvalidParams = -32602,
    InternalError = -32603,
    ServerErrorStart = -32099,
    ServerErrorEnd = -32000,

    /// Error code indicating that a server received a notification or
    /// request before the server has received the `initialize` request.
    ServerNotInitialized = -32002,
    UnknownErrorCode = -32001,

    // Defined by the protocol:
    /// The client has canceled a request and a server has detected
    /// the cancel.
    RequestCanceled = -32800,

    /// The server detected that the content of a document got
    /// modified outside normal conditions. A server should
    /// NOT send this error code if it detects a content change
    /// in it unprocessed messages. The result even computed
    /// on an older state might still be useful for the client.
    ///
    /// If a client decides that a result is not of any use anymore
    /// the client should cancel the request.
    ContentModified = -32801,

    /// The server cancelled the request. This error code should
    /// only be used for requests that explicitly support being
    /// server cancellable.
    ///
    /// @since 3.17.0
    ServerCancelled = -32802,

    /// A request failed but it was syntactically correct, e.g the
    /// method name was known and the parameters were valid. The error
    /// message should contain human readable information about why
    /// the request failed.
    ///
    /// @since 3.17.0
    RequestFailed = -32803,
}
