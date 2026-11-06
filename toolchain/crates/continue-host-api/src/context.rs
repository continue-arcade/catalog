//! Host API 调用者身份与不可变授权快照。

use crate::Capability;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::sync::Arc;

/// 一个由 Host 分配的 session 标识。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SessionId(pub String);

/// 单次 Host API 调用的请求标识。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RequestId(pub String);

/// Host API 调用发起方的身份。
///
/// Extension 身份带有 manifest 中的稳定 ID 和版本；Agent 身份仅带其 agent ID；
/// `Internal` 刻意不携带实现组件名称，避免把内部拓扑带到跨边界契约。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ActorIdentity {
    /// 受限 Extension 调用。
    Extension {
        /// Extension manifest 中声明的稳定 ID。
        #[serde(rename = "extensionId")]
        extension_id: String,
        /// Extension manifest 中声明的版本。
        version: String,
    },
    /// 受 policy 控制的 Agent 调用。
    Agent {
        /// Agent 的稳定身份标识。
        #[serde(rename = "agentId")]
        agent_id: String,
    },
    /// Continue 自身的受信任内部调用。
    Internal,
}

/// 对 mount 的具体读写授权。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileAccess {
    /// 可以读取文件或列举目录。
    Read,
    /// 可以创建或修改文件。
    Write,
}

/// 单一 mount 的文件系统授权。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilesystemPermission {
    /// 被授权的逻辑 mount 名称。
    pub mount: String,
    /// 允许在该 mount 上执行的访问方式。
    pub access: BTreeSet<FileAccess>,
}

impl FilesystemPermission {
    /// library/* 是扫描根集合；其他挂载必须逐项匹配。
    pub fn matches_mount(&self, mount: &str) -> bool {
        self.mount == mount
            || (self.mount == "library/*"
                && mount.starts_with("library/")
                && crate::FileRef::new(mount, "").is_ok())
    }
}

/// 调用者可访问的网络域名集合。
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkPermissionSet {
    /// 精确匹配、已授权的域名。
    #[serde(default)]
    pub allowed_domains: BTreeSet<String>,
}

/// 调用者可读取的密钥名称集合。
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretPermissionSet {
    /// 调用者有权读取的密钥名称。
    #[serde(default)]
    pub allowed_secrets: BTreeSet<String>,
}

/// 单次调用时已经确定的授权快照。
///
/// 它描述调用者可以使用的方法、mount、网络域名和密钥；它不表示当前操作系统
/// 是否具备某项 Host 功能。Host 不能在同一调用中修改该值。
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorizationSnapshot {
    /// 当前调用者被授予的 Host API 方法。
    #[serde(default)]
    pub capabilities: BTreeSet<Capability>,
    /// 当前调用者可访问的 mount 及其读写方式。
    #[serde(default)]
    pub filesystem: Vec<FilesystemPermission>,
    /// 当前调用者可请求的网络域名。
    #[serde(default)]
    pub network: NetworkPermissionSet,
    /// 当前调用者可读取的密钥名称。
    #[serde(default)]
    pub secrets: SecretPermissionSet,
}

/// 一次 Host API 调用的 owned 上下文。
///
/// `authorization` 使用 `Arc` 共享不可变快照，因此并发调用无需复制整个授权集合。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiContext {
    /// 本次调用的发起方。
    pub actor: ActorIdentity,
    /// 已协商的 session；在 hello 前可以为空。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<SessionId>,
    /// 用于关联日志和 transport 请求的可选标识。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<RequestId>,
    /// 调用开始时固定的授权快照。
    pub authorization: Arc<AuthorizationSnapshot>,
    /// 可信 Runtime 注入的本次执行截止时间；不接受 JSON 输入，也不进入 wire contract。
    #[serde(skip)]
    pub execution_deadline: Option<std::time::Instant>,
    /// 可信 Runtime 注入的单次调用普通设置；不接受客户端输入，也不进入 wire。
    #[serde(skip)]
    pub settings: Arc<crate::settings::SettingsValues>,
}

impl ApiContext {
    /// 使用给定 actor 和授权快照创建上下文。
    pub fn new(actor: ActorIdentity, authorization: Arc<AuthorizationSnapshot>) -> Self {
        Self {
            actor,
            session_id: None,
            request_id: None,
            authorization,
            execution_deadline: None,
            settings: Arc::new(Default::default()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ActorIdentity, ApiContext, AuthorizationSnapshot, FileAccess, FilesystemPermission,
        NetworkPermissionSet, RequestId, SecretPermissionSet, SessionId,
    };
    use crate::Capability;
    use serde_json::json;
    use std::collections::BTreeSet;
    use std::sync::Arc;

    #[test]
    fn serializes_context_with_a_shared_authorization_snapshot() {
        let context = ApiContext {
            settings: Arc::new(Default::default()),
            execution_deadline: None,
            actor: ActorIdentity::Extension {
                extension_id: "org.continue.plugin.igdb".to_owned(),
                version: "0.1.0".to_owned(),
            },
            session_id: Some(SessionId("session-1".to_owned())),
            request_id: Some(RequestId("request-1".to_owned())),
            authorization: Arc::new(AuthorizationSnapshot {
                capabilities: BTreeSet::from([Capability::NetRequest, Capability::FsWriteText]),
                filesystem: vec![FilesystemPermission {
                    mount: "extension-cache".to_owned(),
                    access: BTreeSet::from([FileAccess::Read, FileAccess::Write]),
                }],
                network: NetworkPermissionSet {
                    allowed_domains: BTreeSet::from(["api.igdb.com".to_owned()]),
                },
                secrets: SecretPermissionSet {
                    allowed_secrets: BTreeSet::from(["igdb-client-secret".to_owned()]),
                },
            }),
        };

        let value = serde_json::to_value(&context).expect("上下文必须可序列化");
        assert_eq!(value["actor"]["type"], json!("extension"));
        assert_eq!(
            value["actor"]["extensionId"],
            json!("org.continue.plugin.igdb")
        );
        assert_eq!(
            value["authorization"]["capabilities"],
            json!(["net.request", "fs.writeText"])
        );

        let decoded: ApiContext = serde_json::from_value(value).expect("上下文必须可反序列化");
        assert_eq!(decoded, context);
    }
    #[test]
    fn execution_deadline_is_not_a_wire_or_guest_control() {
        let mut context = ApiContext::new(
            ActorIdentity::Internal,
            Arc::new(AuthorizationSnapshot::default()),
        );
        context.execution_deadline = Some(std::time::Instant::now());
        let mut value = serde_json::to_value(&context).unwrap();
        assert!(value.get("executionDeadline").is_none());
        value["executionDeadline"] = json!(999999);
        let decoded: ApiContext = serde_json::from_value(value).unwrap();
        assert!(decoded.execution_deadline.is_none());
    }
}
