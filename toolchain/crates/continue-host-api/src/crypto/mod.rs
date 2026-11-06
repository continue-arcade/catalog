//! `crypto.*` 方法的请求与响应契约。

use crate::FileRef;
use serde::{Deserialize, Serialize};

/// `crypto.hashFile` 的稳定方法名。
pub const HASH_FILE_METHOD: &str = "crypto.hashFile";

/// v0.1 支持的文件 hash 算法。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HashAlgorithm {
    /// CRC-32。
    Crc32,
    /// MD5。
    Md5,
    /// SHA-1。
    Sha1,
    /// SHA-256。
    Sha256,
}

/// `crypto.hashFile` 请求。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HashFileRequest {
    /// 需要读取并计算 hash 的逻辑文件。
    pub file: FileRef,
    /// 调用方要求的 hash 算法。
    pub algorithm: HashAlgorithm,
}

/// `crypto.hashFile` 响应。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HashFileResponse {
    /// 实际执行的算法，与请求中的值一致。
    pub algorithm: HashAlgorithm,
    /// 小写十六进制 hash 值。
    pub value: String,
}
