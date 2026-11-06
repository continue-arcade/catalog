//! `core.*` 方法的请求与响应契约。

use crate::{Capability, SessionId};
use serde::{Deserialize, Serialize};

/// `core.hello` 的稳定方法名。
pub const HELLO_METHOD: &str = "core.hello";
/// `core.capabilities` 的稳定方法名。
pub const CAPABILITIES_METHOD: &str = "core.capabilities";

/// `core.hello` 请求：声明调用方期望协商的 API 版本。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HelloRequest {
    /// 调用方期望使用的 Host API 版本，例如 `0.1`。
    pub api_version: String,
}

/// `core.hello` 响应：确认当前 API 版本并建立 session。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HelloResponse {
    /// Host 实际协商出的 API 版本。
    pub api_version: String,
    /// Host 为本次会话分配的标识。
    pub session_id: SessionId,
}

/// `core.capabilities` 请求。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CapabilitiesRequest {}

/// `core.capabilities` 响应。
///
/// `methods` 是当前 Host 实现并对本次调用可见的方法；`capabilities` 是调用者已获
/// 授的调用权限。两者分开，避免将平台支持误解为 actor 已获授权。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilitiesResponse {
    /// 当前 Host 提供的方法名。
    pub methods: Vec<String>,
    /// 当前调用者已获授权的能力。
    pub capabilities: Vec<Capability>,
}
