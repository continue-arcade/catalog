//! `kv.*` 方法的请求与响应契约。

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// `kv.get` 的稳定方法名。
pub const GET_METHOD: &str = "kv.get";
/// `kv.set` 的稳定方法名。
pub const SET_METHOD: &str = "kv.set";
/// `kv.delete` 的稳定方法名。
pub const DELETE_METHOD: &str = "kv.delete";

/// `kv.get` 请求。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetRequest {
    /// 当前 actor namespace 中的 KV 键。
    pub key: String,
}

/// `kv.get` 响应。
///
/// `None` 表示键不存在；`Some(Value::Null)` 表示键存在且存储的 JSON 值为 null。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetResponse {
    /// 已保存的 JSON 值（若键存在）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
}

/// `kv.set` 请求。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetRequest {
    /// 当前 actor namespace 中的 KV 键。
    pub key: String,
    /// 要持久化的 JSON 值。
    pub value: Value,
}

/// `kv.set` 响应。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SetResponse {}

/// `kv.delete` 请求。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeleteRequest {
    /// 当前 actor namespace 中的 KV 键。
    pub key: String,
}

/// `kv.delete` 响应。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DeleteResponse {}
