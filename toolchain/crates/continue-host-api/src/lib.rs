//! Continue Host API 的平台无关契约。
//!
//! 本 crate 只定义跨边界的请求、响应、上下文和错误类型。文件系统、网络和密钥
//! 的具体实现属于后续的 `continue-host` crate，不能反向成为本 crate 的依赖。

pub mod capability;
pub mod context;
pub mod core;
pub mod crypto;
pub mod error;
pub mod fs;
pub mod kv;
pub mod log;
pub mod net;
pub mod secret;
pub mod settings;
pub mod system;
pub mod types;

pub use capability::Capability;
pub use context::{
    ActorIdentity, ApiContext, AuthorizationSnapshot, FileAccess, FilesystemPermission,
    NetworkPermissionSet, RequestId, SecretPermissionSet, SessionId,
};
pub use error::{ApiError, ApiErrorKind};
pub use types::{FileRef, NormalizedRelativePath};

/// 当前稳定 Host API wire contract 版本。
pub const CONTINUE_HOST_API_VERSION: &str = "0.1";
