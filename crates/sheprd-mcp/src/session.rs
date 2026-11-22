//! Answering one request in a connection's lifecycle, without I/O: the
//! server feeds requests in order and writes what comes back.

use crate::{
    Call, ClientInfo, Handler, MAX_MESSAGE, PROTOCOL_VERSIONS, ServerInfo, ToolResult,
    jsonrpc::{
        self, Failure, INTERNAL_ERROR, INVALID_PARAMS, INVALID_REQUEST, METHOD_NOT_FOUND, Request,
    },
};
use serde_json::{Map, Value, json};

/// The longest tool name quoted back in an error.
const MAX_QUOTED_NAME: usize = 128;

pub(crate) struct Session {
    info: ServerInfo,
    /// Set once `initialize` succeeded; `None` before. The client inside is
    /// `None` when it gave no name.
    client: Option<Option<ClientInfo>>,
}

impl Session {
    pub(crate) fn new(info: ServerInfo) -> Self {
        Self { info, client: None }
    }

    /// The response to `request`, encoded within [`MAX_MESSAGE`]: checked
    /// whole, ID and envelope included, whatever produced it.
    pub(crate) fn answer(&mut self, request: Request, handler: &mut impl Handler) -> Value {
        let id = request.id.clone();
        let response = match self.result(request, handler) {
            Ok(result) => jsonrpc::result(&id, result),
            Err(failure) => jsonrpc::error(Some(&id), &failure),
        };
        if encoded_len(&response) <= MAX_MESSAGE {
            return response;
        }
        let failure = Failure::new(
            INTERNAL_ERROR,
            format!("The answer exceeds {MAX_MESSAGE} bytes"),
        );
        jsonrpc::error(Some(&id), &failure)
    }

    fn result(&mut self, request: Request, handler: &mut impl Handler) -> Result<Value, Failure> {
        let params = request.params.unwrap_or_default();
        match (request.method.as_str(), &self.client) {
            ("ping", _) => Ok(json!({})),
            ("initialize", None) => self.initialize(&params, handler),
            ("initialize", Some(_)) => Err(Failure::new(INVALID_REQUEST, "Already initialized")),
            (_, None) => Err(Failure::new(INVALID_REQUEST, "Not initialized")),
            ("tools/list", Some(_)) => list(&params, handler),
            ("tools/call", Some(client)) => call(params, client.clone(), handler),
            _ => Err(Failure::new(METHOD_NOT_FOUND, "Method not found")),
        }
    }

    fn initialize(
        &mut self,
        params: &Map<String, Value>,
        handler: &mut impl Handler,
    ) -> Result<Value, Failure> {
        let Some(asked) = params.get("protocolVersion").and_then(Value::as_str) else {
            return Err(Failure::new(INVALID_PARAMS, "protocolVersion is required"));
        };
        let version = PROTOCOL_VERSIONS
            .iter()
            .find(|version| **version == asked)
            .or(PROTOCOL_VERSIONS.first())
            .copied()
            .unwrap_or_default();
        let client = params
            .get("clientInfo")
            .and_then(|info| info.get("name"))
            .and_then(Value::as_str)
            .map(ClientInfo::new)
            .filter(|client| !client.name.is_empty());
        self.client = Some(client);
        let mut server = json!({ "name": self.info.name, "version": self.info.version });
        if let Some(title) = &self.info.title {
            server["title"] = json!(title);
        }
        let mut result = json!({
            "protocolVersion": version,
            "capabilities": { "tools": { "listChanged": false } },
            "serverInfo": server,
        });
        if let Some(instructions) = handler.instructions() {
            result["instructions"] = json!(instructions);
        }
        Ok(result)
    }
}

fn list(params: &Map<String, Value>, handler: &mut impl Handler) -> Result<Value, Failure> {
    // Every tool comes in one page, so no cursor of ours exists to continue.
    if params.get("cursor").is_some_and(|cursor| !cursor.is_null()) {
        return Err(Failure::new(INVALID_PARAMS, "Unknown cursor"));
    }
    let tools = handler.tools();
    let value = json!({ "tools": tools });
    if encoded_len(&value) > MAX_MESSAGE {
        return Err(Failure::new(
            INTERNAL_ERROR,
            "The tool list exceeds the message bound",
        ));
    }
    Ok(value)
}

fn call(
    mut params: Map<String, Value>,
    client: Option<ClientInfo>,
    handler: &mut impl Handler,
) -> Result<Value, Failure> {
    let Some(Value::String(name)) = params.remove("name") else {
        return Err(Failure::new(INVALID_PARAMS, "name must be a string"));
    };
    let arguments = match params.remove("arguments") {
        None | Some(Value::Null) => Map::new(),
        Some(Value::Object(arguments)) => arguments,
        Some(_) => return Err(Failure::new(INVALID_PARAMS, "arguments must be an object")),
    };
    let quoted = ClientInfo::new(&name.chars().take(MAX_QUOTED_NAME).collect::<String>()).name;
    let call = Call {
        name,
        arguments,
        client,
    };
    let result = handler
        .call(call)
        .map_err(|_| Failure::new(INVALID_PARAMS, format!("Unknown tool: {quoted}")))?;
    let mut value = serde_json::to_value(&result)
        .map_err(|_| Failure::new(INTERNAL_ERROR, "Could not encode the result"))?;
    // Leave room for the envelope around the result.
    if encoded_len(&value) > MAX_MESSAGE - 1024 {
        let error = ToolResult::error(format!(
            "The answer exceeds {MAX_MESSAGE} bytes; ask for less, such as with a limit"
        ));
        value = serde_json::to_value(&error)
            .map_err(|_| Failure::new(INTERNAL_ERROR, "Could not encode the result"))?;
    }
    Ok(value)
}

fn encoded_len(value: &Value) -> usize {
    serde_json::to_vec(value).map_or(usize::MAX, |bytes| bytes.len())
}
