//! 跨 Host API 边界传递的稳定错误契约。

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;

/// Host API 对调用方公开的稳定错误类别。
///
/// 该枚举的既有语义和字符串表示从 v0.1 起保持稳定；后续版本只能新增类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiErrorKind {
    /// 请求参数不符合对应方法的契约。
    InvalidArgument,
    /// Host 或当前平台不支持该方法或操作。
    Unsupported,
    /// 调用者没有所需 capability 或资源授权。
    PermissionDenied,
    /// 请求的资源不存在。
    NotFound,
    /// 请求创建的资源已经存在。
    AlreadyExists,
    /// 请求与当前资源状态冲突。
    Conflict,
    /// 操作超过了允许的时间。
    Timeout,
    /// 操作被调用者、Host 或平台取消。
    Cancelled,
    /// 远端或本地限流。
    RateLimited,
    /// 请求超过配额或大小限制。
    QuotaExceeded,
    /// 网络连接、DNS 或传输失败。
    NetworkError,
    /// 远端服务返回了错误响应。
    RemoteError,
    /// Host 在本地 I/O 操作中失败。
    IoError,
    /// 未能归类的 Host 内部错误。
    InternalError,
}

impl fmt::Display for ApiErrorKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::InvalidArgument => "invalid_argument",
            Self::Unsupported => "unsupported",
            Self::PermissionDenied => "permission_denied",
            Self::NotFound => "not_found",
            Self::AlreadyExists => "already_exists",
            Self::Conflict => "conflict",
            Self::Timeout => "timeout",
            Self::Cancelled => "cancelled",
            Self::RateLimited => "rate_limited",
            Self::QuotaExceeded => "quota_exceeded",
            Self::NetworkError => "network_error",
            Self::RemoteError => "remote_error",
            Self::IoError => "io_error",
            Self::InternalError => "internal_error",
        };

        formatter.write_str(value)
    }
}

/// 跨 Host API 边界返回的安全错误。
///
/// `message` 只包含可安全公开给调用方的描述；`details` 可以提供结构化、非敏感
/// 上下文，绝不能包含绝对路径、密钥或内部错误链。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, thiserror::Error)]
#[error("{kind}: {message}")]
pub struct ApiError {
    /// 可供调用方程序化处理的稳定错误类别。
    pub kind: ApiErrorKind,
    /// 可安全展示给调用方的人类可读描述。
    pub message: String,
    /// 可选的非敏感结构化上下文。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

impl ApiError {
    /// 创建一个可安全跨边界传递的错误。
    pub fn new(kind: ApiErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            details: None,
        }
    }

    /// 创建包含非敏感结构化细节的错误。
    pub fn with_details(kind: ApiErrorKind, message: impl Into<String>, details: Value) -> Self {
        Self {
            kind,
            message: message.into(),
            details: Some(details),
        }
    }

    /// 创建参数验证失败错误。
    pub fn invalid_argument(message: impl Into<String>) -> Self {
        Self::new(ApiErrorKind::InvalidArgument, message)
    }

    /// 创建方法或平台不支持错误。
    pub fn unsupported(message: impl Into<String>) -> Self {
        Self::new(ApiErrorKind::Unsupported, message)
    }
}

#[cfg(test)]
mod tests {
    use super::{ApiError, ApiErrorKind};
    use serde_json::json;

    #[test]
    fn serializes_the_stable_error_shape() {
        let error = ApiError::with_details(
            ApiErrorKind::QuotaExceeded,
            "Response size exceeds the configured limit",
            json!({ "limit": 8_388_608 }),
        );

        assert_eq!(
            serde_json::to_value(error).expect("ApiError 必须可序列化"),
            json!({
                "kind": "quota_exceeded",
                "message": "Response size exceeds the configured limit",
                "details": { "limit": 8_388_608 }
            })
        );
    }

    #[test]
    fn omits_absent_details() {
        let error = ApiError::invalid_argument("mount must not be empty");

        assert_eq!(
            serde_json::to_value(error).expect("ApiError 必须可序列化"),
            json!({
                "kind": "invalid_argument",
                "message": "mount must not be empty"
            })
        );
    }
}
