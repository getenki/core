use serde_json::{Value, json};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone)]
pub(crate) struct Message {
    pub(crate) message_id: String,
    pub(crate) prev_message_id: Option<String>,
    sender: String,
    payload: Value,
}

pub(crate) struct IndexedValue {
    pub(crate) index: usize,
    pub(crate) value: Value,
}

impl Message {
    pub(crate) fn system(content: String) -> Self {
        Self {
            message_id: Self::next_message_id(),
            prev_message_id: None,
            sender: "system".to_string(),
            payload: json!({
                "role": "system",
                "content": content
            }),
        }
    }

    pub(crate) fn user(
        content: String,
        request_id: String,
        prev_message_id: Option<String>,
        user_id: Option<String>,
    ) -> Self {
        Self {
            message_id: Self::next_message_id(),
            prev_message_id,
            sender: user_id.unwrap_or_else(|| "user".to_string()),
            payload: json!({
                "role": "user",
                "content": content,
                "request_id": request_id
            }),
        }
    }

    pub(crate) fn out(payload: Value, prev_message_id: Option<String>) -> Self {
        let sender = payload
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("assistant")
            .to_string();

        Self {
            message_id: Self::next_message_id(),
            prev_message_id,
            sender,
            payload,
        }
    }

    fn next_message_id() -> String {
        format!("msg-{}", current_timestamp_nanos())
    }
}

impl From<&Message> for Value {
    fn from(message: &Message) -> Self {
        message.payload.clone()
    }
}

impl From<Message> for Value {
    fn from(message: Message) -> Self {
        let mut value = json!({
            "message_id": message.message_id,
            "sender": message.sender,
            "payload": message.payload
        });

        if let Some(prev_message_id) = message.prev_message_id {
            value["prev_message_id"] = Value::String(prev_message_id);
        }

        value
    }
}

impl TryFrom<IndexedValue> for Message {
    type Error = ();

    fn try_from(indexed: IndexedValue) -> Result<Self, Self::Error> {
        let IndexedValue { index, value } = indexed;
        let message_id = value
            .get("message_id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| format!("legacy-msg-{index}"));
        let prev_message_id = value
            .get("prev_message_id")
            .and_then(Value::as_str)
            .map(str::to_string);
        let sender = value
            .get("sender")
            .and_then(Value::as_str)
            .map(str::to_string);

        if let Some(payload) = value.get("payload").cloned() {
            return Ok(Self {
                message_id,
                prev_message_id,
                sender: sender.unwrap_or_else(|| {
                    payload
                        .get("role")
                        .and_then(Value::as_str)
                        .unwrap_or("assistant")
                        .to_string()
                }),
                payload,
            });
        }

        let role = value.get("role").and_then(Value::as_str).ok_or(())?;

        Ok(Self {
            message_id: format!("legacy-msg-{index}"),
            prev_message_id: None,
            sender: role.to_string(),
            payload: value,
        })
    }
}

pub(crate) fn next_request_id() -> String {
    format!("req-{}", current_timestamp_nanos())
}

fn current_timestamp_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default()
}
