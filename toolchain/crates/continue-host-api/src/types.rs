//! 跨平台文件引用与已规范化的相对路径。

use crate::error::ApiError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::str::FromStr;

/// 一个 mount 内的跨平台文件引用。
///
/// `mount` 是权限检查边界，`path` 始终是 Unix 风格的相对路径。请优先使用
/// [`FileRef::new`] 或反序列化构造；两者都会执行相同的输入验证。Host 在解析
/// FileRef 前也必须再次调用 [`FileRef::validate`]，以防原生调用者直接构造字段。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct FileRef {
    /// 命名的、被授权的 Host mount，例如 `extension-cache`。
    pub mount: String,
    /// mount 内使用 `/` 分隔的相对路径；空字符串表示 mount 根目录。
    pub path: String,
}

impl FileRef {
    /// 创建并验证一个跨平台文件引用。
    pub fn new(mount: impl Into<String>, path: impl Into<String>) -> Result<Self, ApiError> {
        let file = Self {
            mount: mount.into(),
            path: path.into(),
        };
        file.validate()?;
        Ok(file)
    }

    /// 验证 mount 名称和相对路径。
    pub fn validate(&self) -> Result<(), ApiError> {
        validate_mount_name(&self.mount)?;
        NormalizedRelativePath::parse(&self.path)?;
        Ok(())
    }

    /// 将已验证的原始路径解析为供 Host backend 使用的段表示。
    pub fn normalized_path(&self) -> Result<NormalizedRelativePath, ApiError> {
        validate_mount_name(&self.mount)?;
        NormalizedRelativePath::parse(&self.path)
    }
}

impl<'de> Deserialize<'de> for FileRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct FileRefWire {
            mount: String,
            path: String,
        }

        let wire = FileRefWire::deserialize(deserializer)?;
        Self::new(wire.mount, wire.path).map_err(serde::de::Error::custom)
    }
}

/// 已验证且仅由相对 Unix 路径段组成的路径。
///
/// 内部以段数组保存，供 iOS bookmark、Android SAF 与桌面 backend 逐段解析；对外
/// 序列化为通常的 `/` 分隔字符串。空路径是 mount 根目录。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NormalizedRelativePath {
    segments: Vec<String>,
}

impl NormalizedRelativePath {
    /// 解析并验证一个 Unix 风格相对路径。
    pub fn parse(value: &str) -> Result<Self, ApiError> {
        if value.contains('\0') {
            return Err(ApiError::invalid_argument(
                "path must not contain NUL characters",
            ));
        }
        if value.contains('\\') {
            return Err(ApiError::invalid_argument(
                "path must use Unix-style '/' separators",
            ));
        }
        if value.starts_with('/') {
            return Err(ApiError::invalid_argument("path must be relative"));
        }
        if value.is_empty() {
            return Ok(Self {
                segments: Vec::new(),
            });
        }

        let mut segments = Vec::new();
        for segment in value.split('/') {
            validate_path_segment(segment)?;
            segments.push(segment.to_owned());
        }

        Ok(Self { segments })
    }

    /// 返回已经验证的路径段，保持其原有顺序。
    pub fn segments(&self) -> impl Iterator<Item = &str> {
        self.segments.iter().map(String::as_str)
    }

    /// 当前路径是否表示 mount 根目录。
    pub fn is_root(&self) -> bool {
        self.segments.is_empty()
    }

    /// 当前路径是否不含任何段，即是否表示 mount 根目录。
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    /// 返回路径段数量。
    pub fn len(&self) -> usize {
        self.segments.len()
    }

    /// 将段重组成 Unix 风格的相对路径字符串。
    pub fn as_unix_path(&self) -> String {
        self.segments.join("/")
    }

    /// 返回在当前路径前插入一个已验证路径段后的新路径。
    ///
    /// Host 用它将 Extension 的可信 identity 映射为 extension-scoped mount 的内部
    /// 路径前缀。prefix 仍按普通路径段验证，因此不能引入 `..`、分隔符或平台路径语义。
    pub fn with_prefix(&self, prefix: &str) -> Result<Self, ApiError> {
        let prefix_path = Self::parse(prefix)?;
        if prefix_path.len() != 1 {
            return Err(ApiError::invalid_argument(
                "path prefix must contain exactly one segment",
            ));
        }
        if self.is_root() {
            return Ok(prefix_path);
        }
        Self::parse(&format!("{prefix}/{}", self.as_unix_path()))
    }
}

impl fmt::Display for NormalizedRelativePath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.as_unix_path())
    }
}

impl FromStr for NormalizedRelativePath {
    type Err = ApiError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

impl Serialize for NormalizedRelativePath {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.as_unix_path())
    }
}

impl<'de> Deserialize<'de> for NormalizedRelativePath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}

fn validate_mount_name(mount: &str) -> Result<(), ApiError> {
    if mount.is_empty() {
        return Err(ApiError::invalid_argument("mount must not be empty"));
    }

    if let Some(id) = mount.strip_prefix("library/") {
        if !id.is_empty()
            && id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
        {
            return Ok(());
        }
        return Err(ApiError::invalid_argument("invalid library mount"));
    }
    if mount
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b':'))
    {
        Ok(())
    } else {
        Err(ApiError::invalid_argument(
            "mount may contain only ASCII letters, digits, '-', '_' and ':'",
        ))
    }
}

fn validate_path_segment(segment: &str) -> Result<(), ApiError> {
    if segment.is_empty() {
        return Err(ApiError::invalid_argument(
            "path must not contain empty segments",
        ));
    }
    if matches!(segment, "." | "..") {
        return Err(ApiError::invalid_argument(
            "path must not contain '.' or '..' segments",
        ));
    }
    if is_windows_drive_prefix(segment) {
        return Err(ApiError::invalid_argument(
            "path must not contain a Windows drive prefix",
        ));
    }
    if segment.chars().any(char::is_control) {
        return Err(ApiError::invalid_argument(
            "path must not contain control characters",
        ));
    }

    Ok(())
}

fn is_windows_drive_prefix(segment: &str) -> bool {
    let bytes = segment.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

#[cfg(test)]
mod tests {
    use super::{FileRef, NormalizedRelativePath};
    use serde_json::json;

    #[test]
    fn parses_relative_paths_into_segments() {
        let path =
            NormalizedRelativePath::parse("PSP/games/mhp3rd.iso").expect("正常相对路径必须可解析");

        assert_eq!(
            path.segments().collect::<Vec<_>>(),
            vec!["PSP", "games", "mhp3rd.iso"]
        );
        assert_eq!(path.to_string(), "PSP/games/mhp3rd.iso");
        assert!(!path.is_root());
    }

    #[test]
    fn accepts_empty_path_as_mount_root() {
        let path = NormalizedRelativePath::parse("").expect("空路径必须表示 mount 根目录");

        assert!(path.is_root());
        assert_eq!(path.len(), 0);
        assert_eq!(path.to_string(), "");
    }

    #[test]
    fn rejects_every_required_path_escape_form() {
        for path in [
            "../../../etc/passwd",
            "/etc/passwd",
            "C:/Windows",
            "\\\\server\\share",
            "foo\\bar",
            "foo\0bar",
            "foo/./bar",
            "foo//bar",
        ] {
            let error = FileRef::new("temp", path).expect_err(path);
            assert_eq!(error.kind, crate::ApiErrorKind::InvalidArgument, "{path}");
        }
    }

    #[test]
    fn rejects_invalid_mount_names() {
        for mount in ["", "extension cache", "../../temp", "temp/"] {
            assert!(FileRef::new(mount, "file.txt").is_err(), "{mount}");
        }
    }

    #[test]
    fn file_ref_deserialization_validates_the_path() {
        let valid: FileRef = serde_json::from_value(json!({
            "mount": "extension-cache",
            "path": "igdb/covers/co1234.jpg"
        }))
        .expect("有效 FileRef 必须可反序列化");
        assert_eq!(valid.path, "igdb/covers/co1234.jpg");

        let invalid = serde_json::from_value::<FileRef>(json!({
            "mount": "extension-cache",
            "path": "../secret"
        }));
        assert!(invalid.is_err());
    }

    #[test]
    fn normalized_path_uses_a_string_wire_format() {
        let path =
            NormalizedRelativePath::parse("igdb/covers/co1234.jpg").expect("有效路径必须可解析");

        assert_eq!(
            serde_json::to_value(&path).expect("路径必须可序列化"),
            json!("igdb/covers/co1234.jpg")
        );
        let decoded: NormalizedRelativePath =
            serde_json::from_value(json!("igdb/covers/co1234.jpg")).expect("路径必须可反序列化");
        assert_eq!(decoded, path);
    }

    #[test]
    fn prefixes_paths_only_with_a_validated_segment() {
        let path =
            NormalizedRelativePath::parse("igdb/covers/co1234.jpg").expect("有效路径必须可解析");

        assert_eq!(
            path.with_prefix("org.continue.plugin.igdb")
                .expect("有效 prefix 必须可解析")
                .to_string(),
            "org.continue.plugin.igdb/igdb/covers/co1234.jpg"
        );
        assert!(path.with_prefix("../other-extension").is_err());
        assert!(path.with_prefix("other/extension").is_err());
    }
}
