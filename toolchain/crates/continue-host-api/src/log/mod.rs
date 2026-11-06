//! `log.*` 方法的请求与响应契约。

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// `log.write` 的稳定方法名。
pub const WRITE_METHOD: &str = "log.write";

/// 调用方可写入的日志级别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    /// 最详细的诊断信息。
    Trace,
    /// 调试信息。
    Debug,
    /// 正常运行信息。
    Info,
    /// 可预期但需关注的情况。
    Warn,
    /// 失败或异常。
    Error,
}

/// `log.write` 请求。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogWriteRequest {
    /// 调用方建议的日志级别；Host 可以按 policy 过滤。
    pub level: LogLevel,
    /// 不得包含密钥、token 或未脱敏个人数据的日志消息。
    pub message: String,
    /// 可选的、非敏感结构化字段。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields: Option<Value>,
}

/// `log.write` 响应。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct LogWriteResponse {}
