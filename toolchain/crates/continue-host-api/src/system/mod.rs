//! `system.*` 方法的请求与响应契约。

use serde::{Deserialize, Serialize};

/// `system.info` 的稳定方法名。
pub const INFO_METHOD: &str = "system.info";

/// Host 当前运行的平台类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostPlatform {
    /// macOS。
    Macos,
    /// Microsoft Windows。
    Windows,
    /// Linux。
    Linux,
    /// Apple iOS。
    Ios,
    /// Android。
    Android,
    /// 未被当前契约枚举的平台。
    Other,
}

/// Host 所在应用的公开信息。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplicationInfo {
    /// 应用产品名。
    pub name: String,
    /// 应用版本。
    pub version: String,
}

/// `system.info` 请求。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SystemInfoRequest {}

/// `system.info` 响应。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemInfoResponse {
    /// 当前 Host 所在平台。
    pub platform: HostPlatform,
    /// 当前 Host API 版本。
    pub api_version: String,
    /// 当前宿主应用的公开信息。
    pub app: ApplicationInfo,
}
