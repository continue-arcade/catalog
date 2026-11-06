//! `net.*` 方法的请求与响应契约。
//!
//! `net.request` 只承载文本或 JSON 响应；二进制下载必须使用 `net.download` 并直接
//! 写入 [`crate::FileRef`]。

use crate::FileRef;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// `net.request` 的稳定方法名。
pub const REQUEST_METHOD: &str = "net.request";
/// `net.download` 的稳定方法名。
pub const DOWNLOAD_METHOD: &str = "net.download";

/// HTTP 请求方法。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    /// HTTP GET。
    #[default]
    Get,
    /// HTTP POST。
    Post,
    /// HTTP PUT。
    Put,
    /// HTTP PATCH。
    Patch,
    /// HTTP DELETE。
    Delete,
    /// HTTP HEAD。
    Head,
    /// HTTP OPTIONS。
    Options,
}

/// 一个可重复、可保序的 HTTP header。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Header {
    /// HTTP header 名称。
    pub name: String,
    /// HTTP header 值。
    pub value: String,
}

/// 一个可重复、可保序的 URL query 参数。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryParam {
    /// 参数名称。
    pub name: String,
    /// 参数值。
    pub value: String,
}

/// `net.request` 对响应体的显式处理方式。
///
/// 调用方必须选择模式；Host 不根据 `Content-Type` 自动猜测。不存在二进制 variant，
/// 因为二进制数据必须流式写入 `net.download` 的目标文件。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResponseMode {
    /// 返回受大小限制的 UTF-8 文本。
    Text,
    /// 返回受大小限制并经 Host 解析的 JSON。
    Json,
    /// 不读取或返回响应正文。
    Discard,
}

/// v0.1 支持的请求正文。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RequestBody {
    /// 原样发送的文本。
    Text {
        /// 文本内容。
        value: String,
        /// 可选 HTTP Content-Type；缺省时由实现使用 `text/plain`。
        #[serde(rename = "contentType", skip_serializing_if = "Option::is_none")]
        content_type: Option<String>,
    },
    /// 由 Host 编码并标记为 `application/json` 的 JSON 值。
    Json {
        /// JSON 正文。
        value: Value,
    },
}

/// `net.request` 的内联响应正文。
///
/// JSON transport 中 `Empty` 为 `null`、`Text` 为字符串、`Json` 为原始 JSON 值，
/// 因此 JSON 模式响应可以直接按其领域字段读取。调用方已知请求的 `response_mode`，
/// 该模式用于消除字符串 JSON 的表面歧义。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum NetResponseBody {
    /// `ResponseMode::Discard` 的空正文。
    Empty,
    /// `ResponseMode::Text` 的 UTF-8 文本。
    Text(String),
    /// `ResponseMode::Json` 的已解析 JSON。
    Json(Value),
}

/// `net.request` 请求。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetRequest {
    /// HTTP 请求方法。
    pub method: HttpMethod,
    /// 绝对 HTTP(S) URL；域名授权由 Host 检查。
    pub url: String,
    /// 请求 headers，保留重复项与顺序。
    #[serde(default)]
    pub headers: Vec<Header>,
    /// URL query 参数，保留重复项与顺序。
    #[serde(default)]
    pub query: Vec<QueryParam>,
    /// 可选文本或 JSON 正文。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<RequestBody>,
    /// 调用方明确指定的响应处理方式。
    pub response_mode: ResponseMode,
    /// 可选的单次请求超时毫秒数。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

/// `net.request` 响应。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetResponse {
    /// HTTP 响应状态码。
    pub status: u16,
    /// 响应 headers，保留重复项与顺序。
    pub headers: Vec<Header>,
    /// 重定向完成后的最终 URL。
    pub final_url: String,
    /// 依照请求 `response_mode` 处理后的正文。
    pub body: NetResponseBody,
}

/// `net.download` 请求。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadRequest {
    /// HTTP 请求方法；省略时使用 GET。
    #[serde(default)]
    pub method: HttpMethod,
    /// 绝对 HTTP(S) URL；域名授权由 Host 检查。
    pub url: String,
    /// 请求 headers，保留重复项与顺序。
    #[serde(default)]
    pub headers: Vec<Header>,
    /// URL query 参数，保留重复项与顺序。
    #[serde(default)]
    pub query: Vec<QueryParam>,
    /// 二进制响应的逻辑写入位置。
    pub destination: FileRef,
    /// 可选的单次请求超时毫秒数。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    /// 目标已存在时是否允许覆盖；省略时为 false。
    #[serde(default)]
    pub overwrite: bool,
}

/// `net.download` 响应。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadResponse {
    /// HTTP 响应状态码。
    pub status: u16,
    /// 响应 headers，保留重复项与顺序。
    pub headers: Vec<Header>,
    /// 重定向完成后的最终 URL。
    pub final_url: String,
    /// 实际写入的逻辑文件引用。
    pub file: FileRef,
    /// 已流式写入的字节数。
    pub bytes_written: u64,
    /// 响应声明的 Content-Type（若有）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    /// 响应声明的 Content-Length（若有）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_length: Option<u64>,
    /// 响应 ETag（若有）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub etag: Option<String>,
    /// 响应 Last-Modified（若有）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{DownloadRequest, NetRequest, RequestBody, ResponseMode};
    use serde_json::json;

    #[test]
    fn serializes_the_igdb_request_shape() {
        let request = NetRequest {
            method: super::HttpMethod::Post,
            url: "https://api.igdb.com/v4/games".to_owned(),
            headers: vec![],
            query: vec![],
            body: Some(RequestBody::Text {
                value: "fields name;".to_owned(),
                content_type: Some("text/plain".to_owned()),
            }),
            response_mode: ResponseMode::Json,
            timeout_ms: None,
        };

        assert_eq!(
            serde_json::to_value(request).expect("请求必须可序列化"),
            json!({
                "method": "POST",
                "url": "https://api.igdb.com/v4/games",
                "headers": [],
                "query": [],
                "body": { "type": "text", "value": "fields name;", "contentType": "text/plain" },
                "responseMode": "json"
            })
        );
    }

    #[test]
    fn download_defaults_to_get_without_overwrite() {
        let request: DownloadRequest = serde_json::from_value(json!({
            "url": "https://images.igdb.com/cover.jpg",
            "destination": { "mount": "extension-cache", "path": "igdb/cover.jpg" }
        }))
        .expect("摘要中的下载请求必须可反序列化");

        assert_eq!(request.method, super::HttpMethod::Get);
        assert!(!request.overwrite);
        assert!(request.headers.is_empty());
        assert!(request.query.is_empty());
    }
}
