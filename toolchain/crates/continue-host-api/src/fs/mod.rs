//! `fs.*` 方法的请求与响应契约。

use crate::FileRef;
use serde::{Deserialize, Serialize};

/// `fs.readText` 的稳定方法名。
pub const READ_TEXT_METHOD: &str = "fs.readText";
/// `fs.writeText` 的稳定方法名。
pub const WRITE_TEXT_METHOD: &str = "fs.writeText";
/// `fs.stat` 的稳定方法名。
pub const STAT_METHOD: &str = "fs.stat";
/// `fs.list` 的稳定方法名。
pub const LIST_METHOD: &str = "fs.list";
/// `fs.mounts` 的稳定方法名。
pub const MOUNTS_METHOD: &str = "fs.mounts";

/// `fs.readText` 请求。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadTextRequest {
    /// 需要读取的逻辑文件引用。
    pub file: FileRef,
}

/// `fs.readText` 响应。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadTextResponse {
    /// UTF-8 文本内容。
    pub content: String,
}

/// `fs.writeText` 请求。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WriteTextRequest {
    /// 需要写入的逻辑文件引用。
    pub file: FileRef,
    /// 待写入的 UTF-8 文本。
    pub content: String,
    /// 当目标不存在时是否允许创建它。
    pub create: bool,
}

/// `fs.writeText` 响应。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct WriteTextResponse {}

/// Host API 对外可见的文件类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileType {
    /// 普通文件。
    File,
    /// 目录。
    Directory,
    /// 后端可见、但不属于普通文件或目录的条目。
    Other,
}

/// `fs.stat` 请求。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatRequest {
    /// 需要查询的逻辑文件引用。
    pub file: FileRef,
}

/// `fs.stat` 响应。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatResponse {
    /// Host API 对外可见的条目类型。
    #[serde(rename = "type")]
    pub kind: FileType,
    /// 字节数；目录的具体语义由 backend 决定。
    pub size: u64,
    /// 可选的 RFC 3339 修改时间字符串。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified: Option<String>,
}

/// `fs.list` 请求。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListRequest {
    /// 需要列举的逻辑目录引用。
    pub directory: FileRef,
}

/// `fs.list` 中的一条目录条目。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEntry {
    /// 条目 basename，不含父目录或分隔符。
    pub name: String,
    /// Host API 对外可见的条目类型。
    #[serde(rename = "type")]
    pub kind: FileType,
    /// 可用时的条目字节数。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// 可用时的 RFC 3339 修改时间字符串。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified: Option<String>,
}

/// `fs.list` 响应。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListResponse {
    /// 目录中的直接子条目。
    pub entries: Vec<FileEntry>,
}

/// `fs.mounts` 请求。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct MountsRequest {}

/// 当前调用者可见的 mount 元数据。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MountInfo {
    /// 逻辑 mount 名称。
    pub name: String,
    /// 当前调用者是否拥有写权限。
    pub writable: bool,
}

/// `fs.mounts` 响应。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MountsResponse {
    /// 当前调用者可见的 mount。
    pub mounts: Vec<MountInfo>,
}
