//! `secret.*` 方法的请求与响应契约。

use serde::{Deserialize, Serialize};

/// `secret.get` 的稳定方法名。
pub const GET_METHOD: &str = "secret.get";

/// `secret.get` 请求。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretGetRequest {
    /// 当前 actor 已获授权读取的密钥名称。
    pub name: String,
}

/// `secret.get` 响应。
///
/// v0.1 直接返回明文。后续 Credential Broker 版本可以在不改变请求身份语义的前提下
/// 替换此响应模型。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretGetResponse {
    /// 当前密钥的明文值。
    pub value: String,
}
