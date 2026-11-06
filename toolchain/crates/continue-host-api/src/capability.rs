//! Host API 方法对应的调用权限标识。

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// P0 Host API 方法的类型安全权限标识。
///
/// 所有 variant 在 JSON 中序列化为对应的点分方法字符串，例如
/// [`Capability::FsReadText`] 为 `"fs.readText"`。未预先登记的名称会保留为
/// [`Capability::Custom`]，以便 Domain API 或未来版本安全演进。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Capability {
    /// `core.hello`
    CoreHello,
    /// `core.capabilities`
    CoreCapabilities,
    /// `system.info`
    SystemInfo,
    /// `net.request`
    NetRequest,
    /// `net.download`
    NetDownload,
    /// `fs.readText`
    FsReadText,
    /// `fs.writeText`
    FsWriteText,
    /// `fs.stat`
    FsStat,
    /// `fs.list`
    FsList,
    /// `fs.mounts`
    FsMounts,
    /// `kv.get`
    KvGet,
    /// `kv.set`
    KvSet,
    /// `kv.delete`
    KvDelete,
    /// `secret.get`
    SecretGet,
    /// `crypto.hashFile`
    CryptoHashFile,
    /// `log.write`
    LogWrite,
    /// 预留给未来或 Domain API 的 capability。
    Custom(String),
}

impl Capability {
    /// 全部已登记 P0 capability 的稳定方法字符串。
    pub const P0_METHODS: [&'static str; 16] = [
        "core.hello",
        "core.capabilities",
        "system.info",
        "net.request",
        "net.download",
        "fs.readText",
        "fs.writeText",
        "fs.stat",
        "fs.list",
        "fs.mounts",
        "kv.get",
        "kv.set",
        "kv.delete",
        "secret.get",
        "crypto.hashFile",
        "log.write",
    ];

    /// 返回该 capability 的点分方法字符串。
    pub fn as_str(&self) -> &str {
        match self {
            Self::CoreHello => "core.hello",
            Self::CoreCapabilities => "core.capabilities",
            Self::SystemInfo => "system.info",
            Self::NetRequest => "net.request",
            Self::NetDownload => "net.download",
            Self::FsReadText => "fs.readText",
            Self::FsWriteText => "fs.writeText",
            Self::FsStat => "fs.stat",
            Self::FsList => "fs.list",
            Self::FsMounts => "fs.mounts",
            Self::KvGet => "kv.get",
            Self::KvSet => "kv.set",
            Self::KvDelete => "kv.delete",
            Self::SecretGet => "secret.get",
            Self::CryptoHashFile => "crypto.hashFile",
            Self::LogWrite => "log.write",
            Self::Custom(value) => value,
        }
    }

    /// 将已知 P0 方法映射为其类型安全 variant，未知方法保留为 `Custom`。
    pub fn from_method(value: impl Into<String>) -> Self {
        match value.into() {
            value if value == "core.hello" => Self::CoreHello,
            value if value == "core.capabilities" => Self::CoreCapabilities,
            value if value == "system.info" => Self::SystemInfo,
            value if value == "net.request" => Self::NetRequest,
            value if value == "net.download" => Self::NetDownload,
            value if value == "fs.readText" => Self::FsReadText,
            value if value == "fs.writeText" => Self::FsWriteText,
            value if value == "fs.stat" => Self::FsStat,
            value if value == "fs.list" => Self::FsList,
            value if value == "fs.mounts" => Self::FsMounts,
            value if value == "kv.get" => Self::KvGet,
            value if value == "kv.set" => Self::KvSet,
            value if value == "kv.delete" => Self::KvDelete,
            value if value == "secret.get" => Self::SecretGet,
            value if value == "crypto.hashFile" => Self::CryptoHashFile,
            value if value == "log.write" => Self::LogWrite,
            value => Self::Custom(value),
        }
    }
}

impl fmt::Display for Capability {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl Serialize for Capability {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Capability {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer).map(Self::from_method)
    }
}

#[cfg(test)]
mod tests {
    use super::Capability;
    use serde_json::json;

    #[test]
    fn serializes_all_p0_capabilities_as_method_names() {
        assert_eq!(Capability::P0_METHODS.len(), 16);

        for method in Capability::P0_METHODS {
            let capability = Capability::from_method(method);
            assert_eq!(capability.as_str(), method);
            assert_eq!(
                serde_json::to_value(&capability).expect("capability 必须可序列化"),
                json!(method)
            );
        }
    }

    #[test]
    fn preserves_unknown_methods_as_custom_capabilities() {
        let capability: Capability = serde_json::from_value(json!("library.scrape"))
            .expect("未知 capability 仍必须可反序列化");

        assert_eq!(capability, Capability::Custom("library.scrape".to_owned()));
        assert_eq!(
            serde_json::to_value(capability).expect("Custom 必须可序列化"),
            json!("library.scrape")
        );
    }
}
