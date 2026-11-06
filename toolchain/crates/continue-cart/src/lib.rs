mod presentation;
pub mod signing;
pub use presentation::{read_bundle_presentation, BundleFile, BundlePage, BundlePresentation};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{self, Read, Seek};
use std::path::{Component, Path, PathBuf};
use std::str::FromStr;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use walkdir::WalkDir;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

pub const CARTFILE_SCHEMA: &str = "continue.cartfile.v1";
pub const RESOURCE_MANIFEST_SCHEMA: &str = "continue.resource.v1";
pub const CART_MARKET_INDEX_SCHEMA: &str = "continue.cart-market-index.v1";
pub const CART_FORMAT: &str = "cart.zip.v1";
pub const RESOURCE_MANIFEST_FILE: &str = "continue.toml";

fn resource_manifest_root(entry: &str) -> Option<&str> {
    let (root, name) = entry
        .rsplit_once('/')
        .map_or(("", entry), |(root, name)| (&entry[..root.len() + 1], name));
    (name == RESOURCE_MANIFEST_FILE).then_some(root)
}

fn resource_manifest_entries(entries: &[String]) -> impl Iterator<Item = &str> {
    entries
        .iter()
        .filter(|entry| resource_manifest_root(entry).is_some())
        .map(String::as_str)
}

#[derive(Debug, thiserror::Error)]
pub enum CartError {
    #[error("cart io error at {path}: {source}")]
    Io { path: PathBuf, source: io::Error },

    #[error("cart toml parse error at {path}: {source}")]
    Toml {
        path: PathBuf,
        source: toml::de::Error,
    },

    #[error("cart zip error at {path}: {source}")]
    Zip {
        path: PathBuf,
        source: zip::result::ZipError,
    },

    #[error("cart json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("invalid cart: {0}")]
    Invalid(String),
}

pub type CartResult<T> = Result<T, CartError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cartfile {
    pub schema: String,
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub publisher: String,
    pub description: String,
    #[serde(default)]
    pub presentation: BTreeMap<String, String>,
    #[serde(default)]
    pub links: BTreeMap<String, String>,
    pub artifact: CartArtifactManifest,
    pub include: CartInclude,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CartArtifactManifest {
    pub format: String,
    #[serde(default)]
    pub target: BTreeMap<String, toml::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CartInclude {
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceManifest {
    pub schema: String,
    #[serde(rename = "type")]
    pub resource_type: ResourceType,
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub license: String,
    pub description: String,
    #[serde(default)]
    pub entrypoint: Option<String>,
    #[serde(default)]
    pub entrypoints: BTreeMap<String, String>,
    #[serde(default)]
    pub requires: Vec<ResourceRequirement>,
    #[serde(default)]
    pub targets: Vec<BTreeMap<String, toml::Value>>,
    #[serde(default)]
    pub plugin: Option<PluginManifest>,
    #[serde(default)]
    pub frame_script: Option<FrameScriptManifest>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, toml::Value>,
}

/// 帧脚本的能力请求和显式资源闭包；本声明本身不构成用户授权。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrameScriptManifest {
    #[serde(default)]
    pub permissions: Vec<FrameScriptPermission>,
    #[serde(default)]
    pub sounds: Vec<String>,
    #[serde(default)]
    pub music: Vec<String>,
    #[serde(default)]
    pub images: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrameScriptPermission {
    ReadMemory,
    WriteMemory,
    Draw,
    LocalStorage,
    Touch,
    Diorama,
    WriteInput,
    Audio,
    Host,
    Rewind,
    Menu,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginManifest {
    #[serde(default)]
    pub commands: Vec<String>,
    #[serde(default)]
    pub runtime: Option<PluginRuntimeManifest>,
    #[serde(default)]
    pub permissions: PluginPermissionsManifest,
    #[serde(default)]
    pub contributes: PluginContributesManifest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginRuntimeManifest {
    pub kind: String,
    #[serde(default)]
    pub abi: Option<String>,
    #[serde(default)]
    pub contract: Option<String>,
    pub host_api: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginPermissionsManifest {
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub filesystem: Vec<PluginFilesystemPermissionManifest>,
    #[serde(default)]
    pub network: PluginNetworkPermissionsManifest,
    #[serde(default)]
    pub secrets: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginFilesystemPermissionManifest {
    pub mount: String,
    pub access: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginNetworkPermissionsManifest {
    #[serde(default)]
    pub allowed_domains: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginContributesManifest {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub settings: Vec<continue_host_api::settings::SettingDefinition>,
    #[serde(default)]
    pub scrapers: Vec<PluginScraperContributionManifest>,
    #[serde(default)]
    pub cart_resolvers: Vec<CartResolverContributionManifest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CartResolverContributionManifest {
    pub id: String,
    pub command: String,
    pub hosts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginScraperContributionManifest {
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub search_command: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub scrape_command: String,
    /// `continue.metadata-scraper.v1` 标准入口；声明时替代 search/scrape 两个命令。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata_command: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supported_settings: Vec<String>,
    /// ADR 0136 刮削器声明，校验规则与 Extension Platform 一致。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub languages: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lookup: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub accepts_identifiers: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub emits_identifiers: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provides: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceRequirement {
    #[serde(rename = "type")]
    pub resource_type: ResourceType,
    pub id: String,
    pub version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceType {
    Content,
    Core,
    Shader,
    Diorama,
    Preset,
    Theme,
    Plugin,
    Tool,
    System,
    Script,
}

impl FromStr for ResourceType {
    type Err = CartError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "content" => Ok(Self::Content),
            "core" => Ok(Self::Core),
            "shader" => Ok(Self::Shader),
            "diorama" => Ok(Self::Diorama),
            "preset" => Ok(Self::Preset),
            "theme" => Ok(Self::Theme),
            "plugin" => Ok(Self::Plugin),
            "tool" => Ok(Self::Tool),
            "system" => Ok(Self::System),
            "script" => Ok(Self::Script),
            _ => Err(CartError::Invalid(format!(
                "unknown resource type: {value}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceTrustClass {
    Official,
    Other,
}

impl FromStr for ResourceTrustClass {
    type Err = CartError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "official" => Ok(Self::Official),
            "other" => Ok(Self::Other),
            _ => Err(CartError::Invalid(format!(
                "unknown resource trust class: {value}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceDiscoveryProjection {
    pub resource_type: ResourceType,
    pub resource_id: String,
    pub name: String,
    pub version: String,
    pub resource_path: String,
    pub trust_class: ResourceTrustClass,
    #[serde(default)]
    pub requires: Vec<ResourceRequirement>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CartInspection {
    pub kind: InspectionKind,
    pub cart: Cartfile,
    pub resources: Vec<ResourceDiscoveryProjection>,
    pub bundle_sha256: Option<String>,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InspectionKind {
    SourceTree,
    Bundle,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackReport {
    pub output: PathBuf,
    pub bundle_sha256: String,
    pub entries: Vec<String>,
    pub resources: Vec<ResourceDiscoveryProjection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyReport {
    pub path: PathBuf,
    pub bundle_sha256: String,
    pub cart: Cartfile,
    pub resources: Vec<ResourceDiscoveryProjection>,
    pub trust_class: ResourceTrustClass,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnpackReport {
    pub input: PathBuf,
    pub output: PathBuf,
    pub entries: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CartMarketIndex {
    pub schema: String,
    pub generated_at: String,
    pub source: CartMarketSource,
    pub carts: Vec<CartIndexEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CartMarketSource {
    pub id: String,
    pub name: String,
    pub trust_class: ResourceTrustClass,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CartIndexEntry {
    pub cart_id: String,
    pub cart_name: String,
    pub description: String,
    pub publisher: String,
    pub releases: Vec<CartReleaseIndexEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CartReleaseIndexEntry {
    pub cart_release: String,
    pub artifacts: Vec<CartArtifactIndexEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CartArtifactIndexEntry {
    pub cart_artifact: String,
    pub format: String,
    pub artifact_target: BTreeMap<String, toml::Value>,
    pub bundle_sha256: String,
    pub signature: BTreeMap<String, serde_json::Value>,
    pub official_trust_evidence: BTreeMap<String, serde_json::Value>,
    pub download: DownloadEntry,
    pub resources: Vec<ResourceDiscoveryProjection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadEntry {
    pub kind: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceResolveRequest {
    pub resource_type: ResourceType,
    pub resource_id: String,
    pub version: String,
    pub requester_trust_class: ResourceTrustClass,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceResolveReport {
    pub requests: Vec<ResourceResolution>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceResolution {
    pub request: ResourceResolveRequest,
    pub state: ResourceResolutionState,
    pub selected: Option<ResourceCandidate>,
    pub candidates: Vec<ResourceCandidate>,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceResolutionState {
    Resolved,
    Unresolved,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceCandidate {
    pub source_id: String,
    pub source_name: String,
    pub source_trust_class: ResourceTrustClass,
    pub cart_id: String,
    pub cart_name: String,
    pub cart_release: String,
    pub cart_artifact: String,
    pub download: DownloadEntry,
    pub resource: ResourceDiscoveryProjection,
}

pub fn inspect_path(path: &Path) -> CartResult<CartInspection> {
    if path.is_dir() {
        inspect_source_tree(path)
    } else {
        inspect_bundle(path)
    }
}

/// 读取并验证 source tree 的 Cart 级 metadata，不要求构建产物已存在。
///
/// Local Development 在执行 build 前使用这个入口；发布 inspect/pack 仍会通过
/// `inspect_source_tree` 验证 entrypoint 已存在且位于 include boundary 内。
pub fn read_source_cartfile(root: &Path) -> CartResult<Cartfile> {
    let cart = read_cartfile_from_dir(root)?;
    validate_cartfile(&cart)?;
    Ok(cart)
}

pub fn inspect_source_tree(root: &Path) -> CartResult<CartInspection> {
    let cart = read_source_cartfile(root)?;
    let entries = collect_source_entries(root, &cart)?;
    let resources = discover_resources_in_source(root, &entries)?;
    Ok(CartInspection {
        kind: InspectionKind::SourceTree,
        cart,
        resources,
        bundle_sha256: None,
        issues: Vec::new(),
    })
}

pub fn inspect_bundle(path: &Path) -> CartResult<CartInspection> {
    let report = verify_bundle(path)?;
    Ok(CartInspection {
        kind: InspectionKind::Bundle,
        cart: report.cart,
        resources: report.resources,
        bundle_sha256: Some(report.bundle_sha256),
        issues: report.issues,
    })
}

/// 在解析 manifest 前冻结并校验期望 checksum，再按 system 数据包上限解析；官方信任仍由调用方验签。
pub fn verify_system_bundle(path: &Path, expected_sha256: &str) -> CartResult<VerifyReport> {
    let metadata = fs::metadata(path).map_err(|source| CartError::Io {
        path: path.into(),
        source,
    })?;
    if !metadata.is_file() || metadata.len() > 8 * 1024 * 1024 {
        return Err(CartError::Invalid(
            "system Cart exceeds bundle size limits".into(),
        ));
    }
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|source| CartError::Io {
            path: path.into(),
            source,
        })?
        .take(8 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|source| CartError::Io {
            path: path.into(),
            source,
        })?;
    let digest = format!("{:x}", Sha256::digest(&bytes));
    if bytes.len() > 8 * 1024 * 1024
        || digest
            != expected_sha256
                .strip_prefix("sha256:")
                .unwrap_or(expected_sha256)
    {
        return Err(CartError::Invalid("system Cart checksum changed".into()));
    }
    let mut archive = ZipArchive::new(io::Cursor::new(bytes)).map_err(|source| CartError::Zip {
        path: path.into(),
        source,
    })?;
    if archive.len() > 2048 {
        return Err(CartError::Invalid("too many system Cart entries".into()));
    }
    let mut total = 0_u64;
    for i in 0..archive.len() {
        let entry = archive.by_index(i).map_err(|source| CartError::Zip {
            path: path.into(),
            source,
        })?;
        total = total.saturating_add(entry.size());
        if total > 48 * 1024 * 1024 || entry.name().ends_with(".toml") && entry.size() > 256 * 1024
        {
            return Err(CartError::Invalid(
                "system Cart exceeds expanded size limits".into(),
            ));
        }
    }
    verify_archive(path, format!("sha256:{digest}"), archive, 256 * 1024)
}

/// 验证应用必需的系统 Catalog 包，包括不可缺少的字典及条目身份。
/// 发布身份仍由调用方的签名或内置精确记录决定。
pub fn verify_system_catalog_bundle(
    path: &Path,
    expected_sha256: &str,
) -> CartResult<VerifyReport> {
    let report = verify_system_bundle(path, expected_sha256)?;
    const CATALOG: &str = "org.continue.resource.system.catalog";
    if report.cart.id != "org.continue.system.catalog"
        || report
            .resources
            .iter()
            .any(|resource| resource.resource_type != ResourceType::System)
        || !report
            .resources
            .iter()
            .any(|resource| resource.resource_id == CATALOG)
    {
        return Err(CartError::Invalid(
            "required system Catalog resource is missing".into(),
        ));
    }
    semver::Version::parse(&report.cart.version)
        .map_err(|_| CartError::Invalid("invalid system Cart SemVer".into()))?;
    for resource in &report.resources {
        semver::Version::parse(&resource.version)
            .map_err(|_| CartError::Invalid("invalid system resource SemVer".into()))?;
        for dependency in &resource.requires {
            validate_version_constraint(&dependency.version)?;
            let requirement = semver::VersionReq::parse(&dependency.version).map_err(|_| {
                CartError::Invalid("invalid system resource dependency SemVer".into())
            })?;
            let mut satisfied = false;
            for candidate in &report.resources {
                let version = semver::Version::parse(&candidate.version)
                    .map_err(|_| CartError::Invalid("invalid system resource SemVer".into()))?;
                if candidate.resource_id == dependency.id
                    && candidate.resource_type == dependency.resource_type
                    && requirement.matches(&version)
                {
                    satisfied = true;
                    break;
                }
            }
            if !satisfied {
                return Err(CartError::Invalid(format!(
                    "unresolved system resource dependency: {}",
                    dependency.id
                )));
            }
        }
    }
    let mut ids = BTreeSet::new();
    for resource in &report.resources {
        if !ids.insert(&resource.resource_id) {
            return Err(CartError::Invalid(
                "duplicate system resource identity".into(),
            ));
        }
        let (schema, maximum) = match resource.resource_id.as_str() {
            CATALOG => ("continue.system.catalog.v1", 32 * 1024 * 1024),
            "org.continue.resource.system.sources" => {
                ("continue.system.sources.v1", 4 * 1024 * 1024)
            }
            "org.continue.resource.system.title-lexicon" => {
                ("continue.system.title-lexicon.v1", 4 * 1024 * 1024)
            }
            _ => return Err(CartError::Invalid("unknown system Catalog resource".into())),
        };
        let bytes = read_system_resource(path, &resource.resource_id, expected_sha256, maximum)?;
        let document: serde_json::Value = serde_json::from_slice(&bytes)?;
        if document["schema"] != schema
            || document["version"].as_str() != Some(resource.version.as_str())
        {
            return Err(CartError::Invalid(
                "system resource schema/version mismatch".into(),
            ));
        }
        if resource.resource_id == CATALOG {
            let datasets = document["datasets"]
                .as_object()
                .ok_or_else(|| CartError::Invalid("system Catalog datasets are missing".into()))?;
            for required in ["genres", "gameplay", "platforms", "companies", "series"] {
                if !datasets
                    .get(required)
                    .is_some_and(serde_json::Value::is_array)
                {
                    return Err(CartError::Invalid(format!(
                        "required Catalog dataset is missing: {required}"
                    )));
                }
            }
            if datasets["genres"].as_array().is_none_or(Vec::is_empty) {
                return Err(CartError::Invalid(
                    "system Catalog genres must not be empty".into(),
                ));
            }
            for entries in datasets.values() {
                let entries = entries
                    .as_array()
                    .ok_or_else(|| CartError::Invalid("invalid Catalog dataset".into()))?;
                let mut entry_ids = BTreeSet::new();
                for entry in entries {
                    let id = entry["id"]
                        .as_str()
                        .filter(|id| !id.is_empty() && id.len() <= 512)
                        .ok_or_else(|| {
                            CartError::Invalid("invalid Catalog entry identity".into())
                        })?;
                    if !entry_ids.insert(id)
                        || entry["name"]
                            .as_str()
                            .is_none_or(|name| name.trim().is_empty())
                        || entry
                            .get("display_name")
                            .is_some_and(|name| !name.is_null() && !name.is_string())
                    {
                        return Err(CartError::Invalid(
                            "invalid or duplicate Catalog entry".into(),
                        ));
                    }
                    if let Some(cover) = entry.get("cover").filter(|value| !value.is_null()) {
                        let member = cover["path"].as_str().ok_or_else(|| {
                            CartError::Invalid("invalid Catalog cover path".into())
                        })?;
                        let sha = cover["sha256"].as_str().ok_or_else(|| {
                            CartError::Invalid("invalid Catalog cover checksum".into())
                        })?;
                        let size = cover["byteSize"]
                            .as_u64()
                            .filter(|size| *size > 0 && *size <= 20 * 1024 * 1024)
                            .ok_or_else(|| {
                                CartError::Invalid("invalid Catalog cover size".into())
                            })?;
                        if !matches!(
                            cover["contentType"].as_str(),
                            Some("image/png" | "image/jpeg" | "image/webp")
                        ) {
                            return Err(CartError::Invalid(
                                "invalid Catalog cover content type".into(),
                            ));
                        }
                        let bytes = read_system_resource_member(
                            path,
                            CATALOG,
                            member,
                            expected_sha256,
                            size,
                        )?;
                        if bytes.len() as u64 != size
                            || format!("{:x}", Sha256::digest(&bytes))
                                != sha.strip_prefix("sha256:").unwrap_or(sha)
                        {
                            return Err(CartError::Invalid(
                                "Catalog cover bytes do not match descriptor".into(),
                            ));
                        }
                        validate_catalog_cover_image(&bytes, cover["contentType"].as_str())?;
                    }
                }
            }
        }
    }
    Ok(report)
}

/// Catalog 封面的专用完整解码校验；不改变已有媒体存储的宽松历史契约。
pub fn validate_catalog_cover_image(bytes: &[u8], content_type: Option<&str>) -> CartResult<()> {
    if bytes.is_empty() || bytes.len() > 20 * 1024 * 1024 {
        return Err(CartError::Invalid(
            "Catalog cover exceeds encoded size limit".into(),
        ));
    }
    let format = image::guess_format(bytes)
        .map_err(|_| CartError::Invalid("invalid Catalog cover image".into()))?;
    let detected = match format {
        image::ImageFormat::Png => "image/png",
        image::ImageFormat::Jpeg => "image/jpeg",
        image::ImageFormat::WebP => "image/webp",
        _ => {
            return Err(CartError::Invalid(
                "unsupported Catalog cover content type".into(),
            ))
        }
    };
    if content_type.is_some_and(|declared| declared != detected) {
        return Err(CartError::Invalid(
            "Catalog cover content type mismatch".into(),
        ));
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(64 * 1024 * 1024);
    let mut reader = image::ImageReader::with_format(io::Cursor::new(bytes), format);
    reader.limits(limits);
    let decoded = reader.decode().map_err(|_| {
        CartError::Invalid("Catalog cover pixels cannot be decoded within limits".into())
    })?;
    if decoded.width() == 0 || decoded.height() == 0 {
        return Err(CartError::Invalid(
            "Catalog cover has empty dimensions".into(),
        ));
    }
    Ok(())
}

/// 有界读取指定 system 资源目录内的成员；不能借由相对路径读取其他资源或本机文件。
pub fn read_system_resource_member(
    path: &Path,
    resource_id: &str,
    member: &str,
    expected_sha256: &str,
    maximum: u64,
) -> CartResult<Vec<u8>> {
    validate_relative_entry_name(member)?;
    if member.contains(':') || member.len() > 4096 {
        return Err(CartError::Invalid(
            "invalid system resource member path".into(),
        ));
    }
    let report = verify_system_bundle(path, expected_sha256)?;
    if report
        .resources
        .iter()
        .filter(|resource| resource.resource_id == resource_id)
        .count()
        != 1
    {
        return Err(CartError::Invalid(
            "ambiguous system resource identity".into(),
        ));
    }
    let resource = report
        .resources
        .iter()
        .find(|resource| {
            resource.resource_id == resource_id && resource.resource_type == ResourceType::System
        })
        .ok_or_else(|| CartError::Invalid("system resource was not found".into()))?;
    let entry = format!("{}{member}", resource.resource_path);
    validate_relative_entry_name(&entry)?;
    // 最深的资源目录拥有成员，不能从外层资源跨进另一个嵌套资源。
    let owner = report
        .resources
        .iter()
        .filter(|candidate| entry.starts_with(&candidate.resource_path))
        .max_by_key(|candidate| candidate.resource_path.len());
    if owner.is_none_or(|owner| owner.resource_id != resource_id) {
        return Err(CartError::Invalid(
            "member belongs to another resource".into(),
        ));
    }
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|source| CartError::Io {
            path: path.into(),
            source,
        })?
        .take(8 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|source| CartError::Io {
            path: path.into(),
            source,
        })?;
    if bytes.len() > 8 * 1024 * 1024
        || format!("{:x}", Sha256::digest(&bytes))
            != expected_sha256
                .strip_prefix("sha256:")
                .unwrap_or(expected_sha256)
    {
        return Err(CartError::Invalid(
            "bundle changed while reading member".into(),
        ));
    }
    let mut archive = ZipArchive::new(io::Cursor::new(bytes)).map_err(|source| CartError::Zip {
        path: path.into(),
        source,
    })?;
    let mut file = archive.by_name(&entry).map_err(|source| CartError::Zip {
        path: path.into(),
        source,
    })?;
    let maximum = maximum.min(20 * 1024 * 1024);
    if file.is_dir() || file.size() > maximum {
        return Err(CartError::Invalid(
            "system resource member exceeds size limit".into(),
        ));
    }
    let mut output = Vec::new();
    file.by_ref()
        .take(maximum + 1)
        .read_to_end(&mut output)
        .map_err(|source| CartError::Io {
            path: path.into(),
            source,
        })?;
    if output.len() as u64 > maximum {
        return Err(CartError::Invalid(
            "system resource member exceeds size limit".into(),
        ));
    }
    Ok(output)
}

pub fn read_system_resource(
    path: &Path,
    resource_id: &str,
    expected_sha256: &str,
    maximum: u64,
) -> CartResult<Vec<u8>> {
    let report = verify_system_bundle(path, expected_sha256)?;
    if report
        .bundle_sha256
        .strip_prefix("sha256:")
        .unwrap_or(&report.bundle_sha256)
        != expected_sha256
            .strip_prefix("sha256:")
            .unwrap_or(expected_sha256)
    {
        return Err(CartError::Invalid("system Cart checksum changed".into()));
    }
    let resource = report
        .resources
        .iter()
        .find(|r| r.resource_id == resource_id && r.resource_type == ResourceType::System)
        .ok_or_else(|| CartError::Invalid("system resource was not found".into()))?;
    let bytes = fs::read(path).map_err(|source| CartError::Io {
        path: path.into(),
        source,
    })?;
    if format!("{:x}", Sha256::digest(&bytes))
        != report
            .bundle_sha256
            .strip_prefix("sha256:")
            .unwrap_or(&report.bundle_sha256)
    {
        return Err(CartError::Invalid("bundle changed while reading".into()));
    }
    let mut archive = ZipArchive::new(io::Cursor::new(bytes)).map_err(|source| CartError::Zip {
        path: path.into(),
        source,
    })?;
    let manifest_path = format!("{}{RESOURCE_MANIFEST_FILE}", resource.resource_path);
    let manifest: ResourceManifest = toml::from_str(&read_zip_text(
        path,
        &mut archive,
        &manifest_path,
        256 * 1024,
    )?)
    .map_err(|source| CartError::Toml {
        path: manifest_path.into(),
        source,
    })?;
    let entrypoint = manifest
        .entrypoint
        .ok_or_else(|| CartError::Invalid("system resource entrypoint is missing".into()))?;
    let entry = format!("{}{entrypoint}", resource.resource_path);
    validate_relative_entry_name(&entry)?;
    let mut file = archive.by_name(&entry).map_err(|source| CartError::Zip {
        path: path.into(),
        source,
    })?;
    if file.size() > maximum {
        return Err(CartError::Invalid("system resource is too large".into()));
    }
    let mut bytes = Vec::new();
    file.by_ref()
        .take(maximum + 1)
        .read_to_end(&mut bytes)
        .map_err(|source| CartError::Io {
            path: path.into(),
            source,
        })?;
    if bytes.len() as u64 > maximum {
        return Err(CartError::Invalid("system resource is too large".into()));
    }
    Ok(bytes)
}

pub fn pack_source_tree(root: &Path, output: &Path) -> CartResult<PackReport> {
    let cart = read_cartfile_from_dir(root)?;
    validate_cartfile(&cart)?;
    let entries = collect_source_entries(root, &cart)?;
    let resources = discover_resources_in_source(root, &entries)?;

    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).map_err(|source| CartError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    let file = File::create(output).map_err(|source| CartError::Io {
        path: output.to_path_buf(),
        source,
    })?;
    let mut writer = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    for entry in &entries {
        let source_path = root.join(to_platform_path(entry));
        writer
            .start_file(entry, options)
            .map_err(|source| CartError::Zip {
                path: output.to_path_buf(),
                source,
            })?;
        let mut input = File::open(&source_path).map_err(|source| CartError::Io {
            path: source_path.clone(),
            source,
        })?;
        io::copy(&mut input, &mut writer).map_err(|source| CartError::Io {
            path: source_path,
            source,
        })?;
    }

    writer.finish().map_err(|source| CartError::Zip {
        path: output.to_path_buf(),
        source,
    })?;

    Ok(PackReport {
        output: output.to_path_buf(),
        bundle_sha256: sha256_file(output)?,
        entries,
        resources,
    })
}

pub fn verify_bundle(path: &Path) -> CartResult<VerifyReport> {
    let bundle_sha256 = sha256_file(path)?;
    let file = File::open(path).map_err(|source| CartError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let archive = ZipArchive::new(file).map_err(|source| CartError::Zip {
        path: path.to_path_buf(),
        source,
    })?;

    verify_archive(path, bundle_sha256, archive, u64::MAX)
}

/// 分发入口先冻结文件，再从同一份 bytes 计算摘要并有界解析；发布信任仍由调用者验签。
pub fn verify_distribution_bundle(path: &Path) -> CartResult<VerifyReport> {
    const MAX_BUNDLE: u64 = 256 * 1024 * 1024;
    const MAX_EXPANDED: u64 = 1024 * 1024 * 1024;
    const MAX_MANIFEST: u64 = 1024 * 1024;
    let file = File::open(path).map_err(|source| CartError::Io {
        path: path.into(),
        source,
    })?;
    let metadata = file.metadata().map_err(|source| CartError::Io {
        path: path.into(),
        source,
    })?;
    if !metadata.is_file() || metadata.len() > MAX_BUNDLE {
        return Err(CartError::Invalid(
            "Cart exceeds distribution bundle size limit".into(),
        ));
    }
    // 匿名临时文件保持私有，后续不重新打开调用方可替换的 path。
    let mut frozen = tempfile::tempfile().map_err(|source| CartError::Io {
        path: path.into(),
        source,
    })?;
    let copied =
        io::copy(&mut file.take(MAX_BUNDLE + 1), &mut frozen).map_err(|source| CartError::Io {
            path: path.into(),
            source,
        })?;
    if copied > MAX_BUNDLE {
        return Err(CartError::Invalid(
            "Cart exceeds distribution bundle size limit".into(),
        ));
    }
    frozen.rewind().map_err(|source| CartError::Io {
        path: path.into(),
        source,
    })?;
    let digest = sha256_reader(path, &mut frozen)?;
    frozen.rewind().map_err(|source| CartError::Io {
        path: path.into(),
        source,
    })?;
    let mut archive = ZipArchive::new(frozen).map_err(|source| CartError::Zip {
        path: path.into(),
        source,
    })?;
    if archive.len() > 8192 {
        return Err(CartError::Invalid(
            "too many distribution Cart entries".into(),
        ));
    }
    let mut expanded = 0_u64;
    for index in 0..archive.len() {
        let entry = archive.by_index(index).map_err(|source| CartError::Zip {
            path: path.into(),
            source,
        })?;
        expanded = expanded.saturating_add(entry.size());
        if expanded > MAX_EXPANDED {
            return Err(CartError::Invalid(
                "Cart exceeds distribution expanded size limit".into(),
            ));
        }
        if (entry.name() == "cartfile.toml" || resource_manifest_root(entry.name()).is_some())
            && entry.size() > MAX_MANIFEST
        {
            return Err(CartError::Invalid("manifest exceeds size limit".into()));
        }
    }
    verify_archive(path, digest, archive, MAX_MANIFEST)
}

fn verify_archive<R: Read + Seek>(
    path: &Path,
    bundle_sha256: String,
    mut archive: ZipArchive<R>,
    maximum_manifest: u64,
) -> CartResult<VerifyReport> {
    let entries = validate_zip_entries(path, &mut archive)?;
    let cartfile_text = read_zip_text(path, &mut archive, "cartfile.toml", maximum_manifest)?;
    let cart: Cartfile = toml::from_str(&cartfile_text).map_err(|source| CartError::Toml {
        path: PathBuf::from("cartfile.toml"),
        source,
    })?;
    validate_cartfile(&cart)?;
    validate_entries_against_include(&entries, &cart)?;
    let resources = discover_resources_in_bundle(path, &mut archive, &entries, maximum_manifest)?;

    Ok(VerifyReport {
        path: path.to_path_buf(),
        bundle_sha256,
        cart,
        resources,
        trust_class: ResourceTrustClass::Other,
        issues: vec![
            "signature verification is not implemented; treating bundle as local/untrusted"
                .to_string(),
        ],
    })
}

pub fn unpack_bundle(input: &Path, output: &Path) -> CartResult<UnpackReport> {
    let _verified = verify_bundle(input)?;
    fs::create_dir_all(output).map_err(|source| CartError::Io {
        path: output.to_path_buf(),
        source,
    })?;

    let file = File::open(input).map_err(|source| CartError::Io {
        path: input.to_path_buf(),
        source,
    })?;
    let mut archive = ZipArchive::new(file).map_err(|source| CartError::Zip {
        path: input.to_path_buf(),
        source,
    })?;

    let mut entries = Vec::new();
    for index in 0..archive.len() {
        let mut file = archive.by_index(index).map_err(|source| CartError::Zip {
            path: input.to_path_buf(),
            source,
        })?;
        let name = file.name().to_string();
        validate_relative_entry_name(&name)?;
        let output_path = output.join(to_platform_path(&name));
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent).map_err(|source| CartError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let mut destination = File::create(&output_path).map_err(|source| CartError::Io {
            path: output_path.clone(),
            source,
        })?;
        io::copy(&mut file, &mut destination).map_err(|source| CartError::Io {
            path: output_path,
            source,
        })?;
        entries.push(name);
    }

    Ok(UnpackReport {
        input: input.to_path_buf(),
        output: output.to_path_buf(),
        entries,
    })
}

pub fn generate_market_index(
    paths: &[PathBuf],
    source: CartMarketSource,
) -> CartResult<CartMarketIndex> {
    let mut carts = Vec::new();
    for path in paths {
        let (inspection, bundle_sha256) = if path.is_dir() {
            let temp_path = tempfile::Builder::new()
                .prefix("continue-cart-index-")
                .suffix(".cart")
                .tempfile()
                .map_err(|source| CartError::Io {
                    path: std::env::temp_dir(),
                    source,
                })?
                .into_temp_path();
            let report = pack_source_tree(path, &temp_path)?;
            let inspection = inspect_bundle(&temp_path)?;
            (inspection, report.bundle_sha256)
        } else {
            let inspection = inspect_bundle(path)?;
            let bundle_sha256 = inspection.bundle_sha256.clone().ok_or_else(|| {
                CartError::Invalid("bundle inspection did not produce checksum".to_string())
            })?;
            (inspection, bundle_sha256)
        };

        let cart_artifact = artifact_code(&inspection.cart);
        let artifact = CartArtifactIndexEntry {
            cart_artifact,
            format: inspection.cart.artifact.format.clone(),
            artifact_target: inspection.cart.artifact.target.clone(),
            bundle_sha256,
            signature: BTreeMap::from([
                (
                    "kind".to_string(),
                    serde_json::Value::String("placeholder".to_string()),
                ),
                (
                    "state".to_string(),
                    serde_json::Value::String("not_implemented".to_string()),
                ),
            ]),
            official_trust_evidence: BTreeMap::new(),
            download: DownloadEntry {
                kind: if path.is_dir() {
                    "local_source".to_string()
                } else {
                    "local_file".to_string()
                },
                url: path.to_string_lossy().to_string(),
            },
            resources: inspection.resources,
        };
        carts.push(CartIndexEntry {
            cart_id: inspection.cart.id.clone(),
            cart_name: inspection.cart.name.clone(),
            description: inspection.cart.description.clone(),
            publisher: inspection.cart.publisher.clone(),
            releases: vec![CartReleaseIndexEntry {
                cart_release: inspection.cart.version.clone(),
                artifacts: vec![artifact],
            }],
        });
    }

    let generated_at = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|error| CartError::Invalid(format!("failed to format generated_at: {error}")))?;

    Ok(CartMarketIndex {
        schema: CART_MARKET_INDEX_SCHEMA.to_string(),
        generated_at,
        source,
        carts,
    })
}

pub fn resolve_resources(
    indexes: &[CartMarketIndex],
    requests: &[ResourceResolveRequest],
) -> CartResult<ResourceResolveReport> {
    let mut resolutions = Vec::new();
    for request in requests {
        validate_reverse_dns_id("resource id", &request.resource_id)?;
        validate_version_constraint(&request.version)?;

        let mut candidates = Vec::new();
        for index in indexes {
            validate_market_index(index)?;
            for cart in &index.carts {
                for release in &cart.releases {
                    for artifact in &release.artifacts {
                        for resource in &artifact.resources {
                            if resource.resource_type != request.resource_type
                                || resource.resource_id != request.resource_id
                                || !version_satisfies(&resource.version, &request.version)?
                            {
                                continue;
                            }
                            candidates.push(ResourceCandidate {
                                source_id: index.source.id.clone(),
                                source_name: index.source.name.clone(),
                                source_trust_class: index.source.trust_class,
                                cart_id: cart.cart_id.clone(),
                                cart_name: cart.cart_name.clone(),
                                cart_release: release.cart_release.clone(),
                                cart_artifact: artifact.cart_artifact.clone(),
                                download: artifact.download.clone(),
                                resource: resource.clone(),
                            });
                        }
                    }
                }
            }
        }

        let selected = candidates
            .iter()
            .find(|candidate| {
                trust_allows(
                    request.requester_trust_class,
                    candidate.resource.trust_class,
                )
            })
            .cloned();

        let mut issues = Vec::new();
        if candidates.is_empty() {
            issues.push(format!(
                "no candidate found for {:?} {} {}",
                request.resource_type, request.resource_id, request.version
            ));
        } else if selected.is_none() {
            issues.push(
                "matching candidates exist, but trust policy does not allow them".to_string(),
            );
        }

        resolutions.push(ResourceResolution {
            request: request.clone(),
            state: if selected.is_some() {
                ResourceResolutionState::Resolved
            } else {
                ResourceResolutionState::Unresolved
            },
            selected,
            candidates,
            issues,
        });
    }

    Ok(ResourceResolveReport {
        requests: resolutions,
    })
}

fn read_cartfile_from_dir(root: &Path) -> CartResult<Cartfile> {
    let path = root.join("cartfile.toml");
    let text = fs::read_to_string(&path).map_err(|source| CartError::Io {
        path: path.clone(),
        source,
    })?;
    toml::from_str(&text).map_err(|source| CartError::Toml { path, source })
}

fn discover_resources_in_source(
    root: &Path,
    entries: &[String],
) -> CartResult<Vec<ResourceDiscoveryProjection>> {
    let mut resources = Vec::new();
    for entry in resource_manifest_entries(entries) {
        let path = root.join(to_platform_path(entry));
        let text = fs::read_to_string(&path).map_err(|source| CartError::Io {
            path: path.clone(),
            source,
        })?;
        let manifest: ResourceManifest =
            toml::from_str(&text).map_err(|source| CartError::Toml {
                path: path.clone(),
                source,
            })?;
        validate_resource_manifest(&manifest)?;
        validate_bundle_entrypoints(entry, &manifest, entries)?;
        resources.push(resource_projection(entry, manifest));
    }
    resources.sort_by(|left, right| left.resource_id.cmp(&right.resource_id));
    Ok(resources)
}

fn validate_bundle_entrypoints(
    manifest_entry: &str,
    manifest: &ResourceManifest,
    entries: &[String],
) -> CartResult<()> {
    let resource_root = resource_manifest_root(manifest_entry).unwrap_or(manifest_entry);
    let entrypoints = manifest
        .entrypoint
        .iter()
        .chain(manifest.entrypoints.values())
        .chain(manifest.frame_script.iter().flat_map(|script| {
            script
                .sounds
                .iter()
                .chain(script.music.iter())
                .chain(script.images.values())
        }));
    for entrypoint in entrypoints {
        validate_relative_entry_name(entrypoint)?;
        let bundled_path = format!("{resource_root}{entrypoint}");
        if !entries.iter().any(|entry| entry == &bundled_path) {
            return Err(CartError::Invalid(format!(
                "resource entrypoint is not present in the Cart bundle: {bundled_path}"
            )));
        }
    }
    Ok(())
}

fn discover_resources_in_bundle<R: Read + Seek>(
    bundle_path: &Path,
    archive: &mut ZipArchive<R>,
    entries: &[String],
    maximum_manifest: u64,
) -> CartResult<Vec<ResourceDiscoveryProjection>> {
    let mut resources = Vec::new();
    for entry in resource_manifest_entries(entries) {
        let text = read_zip_text(bundle_path, archive, entry, maximum_manifest)?;
        let manifest: ResourceManifest =
            toml::from_str(&text).map_err(|source| CartError::Toml {
                path: PathBuf::from(entry),
                source,
            })?;
        validate_resource_manifest(&manifest)?;
        validate_bundle_entrypoints(entry, &manifest, entries)?;
        resources.push(resource_projection(entry, manifest));
    }
    resources.sort_by(|left, right| left.resource_id.cmp(&right.resource_id));
    Ok(resources)
}

fn resource_projection(entry: &str, manifest: ResourceManifest) -> ResourceDiscoveryProjection {
    let resource_path = resource_manifest_root(entry).unwrap_or(entry).to_string();
    ResourceDiscoveryProjection {
        resource_type: manifest.resource_type,
        resource_id: manifest.id,
        name: manifest.name,
        version: manifest.version,
        resource_path,
        trust_class: ResourceTrustClass::Other,
        requires: manifest.requires,
    }
}

fn collect_source_entries(root: &Path, cart: &Cartfile) -> CartResult<Vec<String>> {
    let mut entries = vec!["cartfile.toml".to_string()];
    let mut seen = BTreeSet::from(["cartfile.toml".to_string()]);
    let mut seen_lower = BTreeSet::from(["cartfile.toml".to_string()]);

    for include_path in &cart.include.paths {
        validate_include_path(include_path)?;
        let absolute = root.join(to_platform_path(include_path));
        let metadata = fs::symlink_metadata(&absolute).map_err(|source| CartError::Io {
            path: absolute.clone(),
            source,
        })?;
        if metadata.file_type().is_symlink() {
            return Err(CartError::Invalid(format!(
                "include path cannot be a symlink: {include_path}"
            )));
        }
        if metadata.is_file() {
            push_source_entry(
                include_path.trim_end_matches('/'),
                &mut entries,
                &mut seen,
                &mut seen_lower,
            )?;
            continue;
        }
        if !metadata.is_dir() {
            return Err(CartError::Invalid(format!(
                "include path is neither file nor directory: {include_path}"
            )));
        }
        if !include_path.ends_with('/') {
            return Err(CartError::Invalid(format!(
                "directory include path must end with '/': {include_path}"
            )));
        }
        for entry in WalkDir::new(&absolute)
            .follow_links(false)
            .sort_by_file_name()
        {
            let entry =
                entry.map_err(|error| CartError::Invalid(format!("walkdir error: {error}")))?;
            let file_type = entry.file_type();
            let path = entry.path();
            let relative = path.strip_prefix(root).map_err(|error| {
                CartError::Invalid(format!(
                    "failed to compute relative path for {}: {error}",
                    path.display()
                ))
            })?;
            let relative_name = to_zip_name(relative)?;
            if file_type.is_symlink() {
                return Err(CartError::Invalid(format!(
                    "symlink entries are not allowed: {relative_name}"
                )));
            }
            if file_type.is_dir() {
                continue;
            }
            if !file_type.is_file() {
                return Err(CartError::Invalid(format!(
                    "unsupported filesystem entry: {relative_name}"
                )));
            }
            push_source_entry(&relative_name, &mut entries, &mut seen, &mut seen_lower)?;
        }
    }

    Ok(entries)
}

fn push_source_entry(
    entry: &str,
    entries: &mut Vec<String>,
    seen: &mut BTreeSet<String>,
    seen_lower: &mut BTreeSet<String>,
) -> CartResult<()> {
    validate_relative_entry_name(entry)?;
    if !seen.insert(entry.to_string()) {
        return Err(CartError::Invalid(format!("duplicate entry: {entry}")));
    }
    let lower = entry.to_ascii_lowercase();
    if !seen_lower.insert(lower) {
        return Err(CartError::Invalid(format!(
            "case-insensitive path conflict: {entry}"
        )));
    }
    entries.push(entry.to_string());
    Ok(())
}

fn validate_zip_entries<R: Read + Seek>(
    path: &Path,
    archive: &mut ZipArchive<R>,
) -> CartResult<Vec<String>> {
    let mut entries = Vec::new();
    let mut seen = BTreeSet::new();
    let mut seen_lower = BTreeSet::new();
    let mut cartfile_count = 0;

    for index in 0..archive.len() {
        let file = archive.by_index(index).map_err(|source| CartError::Zip {
            path: path.to_path_buf(),
            source,
        })?;
        let name = file.name().to_string();
        validate_relative_entry_name(&name)?;
        if name.ends_with('/') {
            return Err(CartError::Invalid(format!(
                "explicit directory zip entries are not allowed: {name}"
            )));
        }
        if let Some(mode) = file.unix_mode() {
            if mode & 0o170000 == 0o120000 {
                return Err(CartError::Invalid(format!(
                    "symlink zip entries are not allowed: {name}"
                )));
            }
        }
        if !seen.insert(name.clone()) {
            return Err(CartError::Invalid(format!("duplicate zip entry: {name}")));
        }
        let lower = name.to_ascii_lowercase();
        if !seen_lower.insert(lower) {
            return Err(CartError::Invalid(format!(
                "case-insensitive zip path conflict: {name}"
            )));
        }
        if name == "cartfile.toml" {
            cartfile_count += 1;
        }
        entries.push(name);
    }

    if cartfile_count != 1 {
        return Err(CartError::Invalid(format!(
            "zip root must contain exactly one cartfile.toml, found {cartfile_count}"
        )));
    }

    Ok(entries)
}

fn validate_entries_against_include(entries: &[String], cart: &Cartfile) -> CartResult<()> {
    for entry in entries {
        if entry == "cartfile.toml" {
            continue;
        }
        if !cart
            .include
            .paths
            .iter()
            .any(|include| include_covers(include, entry))
        {
            return Err(CartError::Invalid(format!(
                "zip entry is outside include paths: {entry}"
            )));
        }
    }
    Ok(())
}

fn include_covers(include: &str, entry: &str) -> bool {
    if include.ends_with('/') {
        entry.starts_with(include)
    } else {
        entry == include
    }
}

fn read_zip_text<R: Read + Seek>(
    bundle_path: &Path,
    archive: &mut ZipArchive<R>,
    name: &str,
    maximum: u64,
) -> CartResult<String> {
    let mut file = archive.by_name(name).map_err(|source| CartError::Zip {
        path: bundle_path.to_path_buf(),
        source,
    })?;
    if file.size() > maximum {
        return Err(CartError::Invalid("manifest exceeds size limit".into()));
    }
    let mut text = String::new();
    file.by_ref()
        .take(maximum.saturating_add(1))
        .read_to_string(&mut text)
        .map_err(|source| CartError::Io {
            path: PathBuf::from(name),
            source,
        })?;
    if text.len() as u64 > maximum {
        return Err(CartError::Invalid("manifest exceeds size limit".into()));
    }
    Ok(text)
}

fn validate_cartfile(cart: &Cartfile) -> CartResult<()> {
    if cart.schema != CARTFILE_SCHEMA {
        return Err(CartError::Invalid(format!(
            "cartfile schema must be {CARTFILE_SCHEMA}, got {}",
            cart.schema
        )));
    }
    validate_reverse_dns_id("cart id", &cart.id)?;
    validate_semver("cart version", &cart.version)?;
    if cart.name.trim().is_empty() {
        return Err(CartError::Invalid("cart name is required".to_string()));
    }
    if cart.artifact.format != CART_FORMAT {
        return Err(CartError::Invalid(format!(
            "artifact format must be {CART_FORMAT}, got {}",
            cart.artifact.format
        )));
    }
    if cart.include.paths.is_empty() {
        return Err(CartError::Invalid(
            "include.paths must not be empty".to_string(),
        ));
    }
    for path in &cart.include.paths {
        validate_include_path(path)?;
    }
    Ok(())
}

fn validate_resource_manifest(manifest: &ResourceManifest) -> CartResult<()> {
    if manifest.schema != RESOURCE_MANIFEST_SCHEMA {
        return Err(CartError::Invalid(format!(
            "continue.toml resource schema must be {RESOURCE_MANIFEST_SCHEMA}, got {}",
            manifest.schema
        )));
    }
    validate_reverse_dns_id("resource id", &manifest.id)?;
    validate_semver("resource version", &manifest.version)?;
    if manifest.name.trim().is_empty() {
        return Err(CartError::Invalid(format!(
            "resource name is required: {}",
            manifest.id
        )));
    }
    match manifest.resource_type {
        ResourceType::Script => {
            let script = manifest.frame_script.as_ref().ok_or_else(|| {
                CartError::Invalid("script resource must contain [frame_script] metadata".into())
            })?;
            let source = manifest.entrypoint.as_deref().ok_or_else(|| {
                CartError::Invalid("script resource must declare a source entrypoint".into())
            })?;
            if !source.ends_with(".js") || !manifest.entrypoints.is_empty() {
                return Err(CartError::Invalid(
                    "script entrypoint must be JavaScript source".into(),
                ));
            }
            validate_frame_script_path(source)?;
            validate_frame_script(script)?;
        }
        ResourceType::Plugin => {
            let Some(plugin) = &manifest.plugin else {
                return Err(CartError::Invalid(format!(
                    "plugin resource must contain [plugin] metadata: {}",
                    manifest.id
                )));
            };
            if manifest.entrypoint.is_none() {
                return Err(CartError::Invalid(format!(
                    "plugin resource must declare entrypoint: {}",
                    manifest.id
                )));
            }
            validate_plugin_manifest(&manifest.id, plugin)?;
        }
        _ => {}
    }
    if manifest.resource_type != ResourceType::Plugin && manifest.plugin.is_some() {
        return Err(CartError::Invalid(format!(
            "non-plugin resource must not contain [plugin] metadata: {}",
            manifest.id
        )));
    }
    if manifest.resource_type != ResourceType::Script && manifest.frame_script.is_some() {
        return Err(CartError::Invalid(
            "non-script resource must not contain [frame_script] metadata".into(),
        ));
    }
    for requirement in &manifest.requires {
        validate_reverse_dns_id("requirement id", &requirement.id)?;
        validate_version_constraint(&requirement.version)?;
    }
    Ok(())
}

fn validate_frame_script_path(path: &str) -> CartResult<()> {
    if path.len() > 1024
        || path.contains([':', '\0'])
        || path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(CartError::Invalid(
            "invalid frame script resource path".into(),
        ));
    }
    validate_relative_entry_name(path)
}

fn validate_frame_script(script: &FrameScriptManifest) -> CartResult<()> {
    let permissions = script.permissions.iter().copied().collect::<BTreeSet<_>>();
    if permissions.len() != script.permissions.len()
        || (permissions.contains(&FrameScriptPermission::WriteMemory)
            && !permissions.contains(&FrameScriptPermission::ReadMemory))
        || ((!script.sounds.is_empty() || !script.music.is_empty())
            && !permissions.contains(&FrameScriptPermission::Audio))
        || (!script.images.is_empty() && !permissions.contains(&FrameScriptPermission::Draw))
        || script
            .sounds
            .len()
            .checked_add(script.music.len())
            .and_then(|count| count.checked_add(script.images.len()))
            .is_none_or(|count| count > 255)
    {
        return Err(CartError::Invalid(
            "invalid frame script permissions or asset count".into(),
        ));
    }
    let mut paths = BTreeSet::new();
    for path in script.sounds.iter().chain(&script.music) {
        validate_frame_script_path(path)?;
        if !path.starts_with("audio/")
            || !(path.ends_with(".ogg") || path.ends_with(".wav"))
            || !paths.insert(path)
        {
            return Err(CartError::Invalid(
                "invalid frame script audio asset".into(),
            ));
        }
    }
    for (id, path) in &script.images {
        validate_frame_script_path(path)?;
        if id.is_empty()
            || id.len() > 128
            || id == "."
            || id == ".."
            || !id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.'))
            || !path.ends_with(".png")
            || !paths.insert(path)
        {
            return Err(CartError::Invalid(
                "invalid frame script image asset".into(),
            ));
        }
    }
    Ok(())
}

fn validate_plugin_manifest(resource_id: &str, plugin: &PluginManifest) -> CartResult<()> {
    continue_host_api::settings::validate_definitions(&plugin.contributes.settings)
        .map_err(CartError::Invalid)?;
    for field in plugin
        .contributes
        .settings
        .iter()
        .filter(|field| field.is_credential())
    {
        if !plugin
            .permissions
            .capabilities
            .iter()
            .any(|v| v == "secret.get")
            || !plugin.permissions.secrets.contains(&field.key)
        {
            return Err(CartError::Invalid(
                "credential settings require declared secret permissions".into(),
            ));
        }
    }
    let mut resolver_ids = BTreeSet::new();
    for resolver in &plugin.contributes.cart_resolvers {
        if resolver.id.is_empty()
            || resolver.id.contains(':')
            || !resolver_ids.insert(&resolver.id)
            || !plugin.commands.contains(&resolver.command)
            || resolver.hosts.is_empty()
            || resolver.hosts.len() > 16
            || resolver.hosts.iter().any(|host| {
                host.contains(['/', ':', '*'])
                    || !plugin.permissions.network.allowed_domains.contains(host)
            })
        {
            return Err(CartError::Invalid(
                "invalid cart resolver contribution".into(),
            ));
        }
    }
    let command_set = plugin.commands.iter().collect::<BTreeSet<_>>();
    if plugin.commands.is_empty()
        || command_set.len() != plugin.commands.len()
        || plugin
            .commands
            .iter()
            .any(|command| command.trim().is_empty())
    {
        return Err(CartError::Invalid(format!(
            "plugin commands must be unique and non-empty: {resource_id}"
        )));
    }
    let runtime = plugin
        .runtime
        .as_ref()
        .ok_or_else(|| CartError::Invalid(format!("plugin runtime is required: {resource_id}")))?;
    if runtime.host_api != "0.1" {
        return Err(CartError::Invalid(format!(
            "plugin Host API must be 0.1: {resource_id}"
        )));
    }
    match runtime.kind.as_str() {
        "javascript"
            if runtime.contract.as_deref() == Some("continue-js-1") && runtime.abi.is_none() => {}
        "wasm"
            if runtime.abi.as_deref() == Some("continue-wasm-1") && runtime.contract.is_none() => {}
        "javascript" => {
            return Err(CartError::Invalid(format!(
                "JavaScript plugin must declare only contract = continue-js-1: {resource_id}"
            )))
        }
        "wasm" => {
            return Err(CartError::Invalid(format!(
                "WASM plugin must declare only abi = continue-wasm-1: {resource_id}"
            )))
        }
        other => {
            return Err(CartError::Invalid(format!(
                "unsupported plugin runtime {other}: {resource_id}"
            )))
        }
    }
    let mut mounts = BTreeSet::new();
    for filesystem in &plugin.permissions.filesystem {
        if filesystem.mount.trim().is_empty() || !mounts.insert(&filesystem.mount) {
            return Err(CartError::Invalid(format!(
                "plugin filesystem mounts must be unique and non-empty: {resource_id}"
            )));
        }
        if filesystem.access.is_empty()
            || has_duplicate_strings(&filesystem.access)
            || filesystem
                .access
                .iter()
                .any(|access| !matches!(access.as_str(), "read" | "write"))
        {
            return Err(CartError::Invalid(format!(
                "plugin filesystem access must contain only read/write: {resource_id}"
            )));
        }
    }
    if plugin
        .permissions
        .capabilities
        .iter()
        .chain(plugin.permissions.network.allowed_domains.iter())
        .chain(plugin.permissions.secrets.iter())
        .any(|value| value.trim().is_empty())
    {
        return Err(CartError::Invalid(format!(
            "plugin permission values must not be empty: {resource_id}"
        )));
    }
    if has_duplicate_strings(&plugin.permissions.capabilities)
        || has_duplicate_strings(&plugin.permissions.secrets)
        || has_duplicate_strings(&plugin.permissions.network.allowed_domains)
    {
        return Err(CartError::Invalid(format!(
            "plugin permission values must be unique: {resource_id}"
        )));
    }
    let mut scraper_ids = BTreeSet::new();
    for scraper in &plugin.contributes.scrapers {
        if scraper.id.trim().is_empty() || !scraper_ids.insert(&scraper.id) {
            return Err(CartError::Invalid(format!(
                "scraper contribution IDs must be unique and non-empty: {resource_id}"
            )));
        }
        let commands_declared = match &scraper.metadata_command {
            Some(command) => command_set.contains(command),
            None => {
                command_set.contains(&scraper.search_command)
                    && command_set.contains(&scraper.scrape_command)
            }
        };
        if !commands_declared {
            return Err(CartError::Invalid(format!(
                "scraper contribution must reference declared commands: {resource_id}/{}",
                scraper.id
            )));
        }
    }
    Ok(())
}

fn has_duplicate_strings(values: &[String]) -> bool {
    values.iter().collect::<BTreeSet<_>>().len() != values.len()
}

fn validate_include_path(path: &str) -> CartResult<()> {
    if path.is_empty() {
        return Err(CartError::Invalid(
            "include path cannot be empty".to_string(),
        ));
    }
    validate_relative_entry_name(path.trim_end_matches('/'))?;
    if path == "cartfile.toml" {
        return Err(CartError::Invalid(
            "cartfile.toml is included implicitly and must not appear in include.paths".to_string(),
        ));
    }
    Ok(())
}

fn validate_relative_entry_name(name: &str) -> CartResult<()> {
    if name.is_empty() {
        return Err(CartError::Invalid("path cannot be empty".to_string()));
    }
    if name.starts_with('/') {
        return Err(CartError::Invalid(format!(
            "absolute paths are not allowed: {name}"
        )));
    }
    if name.contains('\\') {
        return Err(CartError::Invalid(format!(
            "windows path separators are not allowed: {name}"
        )));
    }
    let path = Path::new(name);
    for component in path.components() {
        match component {
            Component::Normal(_) => {}
            Component::CurDir
            | Component::ParentDir
            | Component::RootDir
            | Component::Prefix(_) => {
                return Err(CartError::Invalid(format!("unsafe path component: {name}")));
            }
        }
    }
    Ok(())
}

fn validate_reverse_dns_id(label: &str, value: &str) -> CartResult<()> {
    let parts: Vec<_> = value.split('.').collect();
    if parts.len() < 2 {
        return Err(CartError::Invalid(format!(
            "{label} must be reverse-DNS-like: {value}"
        )));
    }
    for part in parts {
        if part.is_empty()
            || part.starts_with('-')
            || part.ends_with('-')
            || !part
                .chars()
                .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
        {
            return Err(CartError::Invalid(format!(
                "{label} contains invalid segment: {value}"
            )));
        }
    }
    Ok(())
}

fn validate_semver(label: &str, value: &str) -> CartResult<()> {
    let core = value.split_once('-').map_or(value, |(left, _)| left);
    let core = core.split_once('+').map_or(core, |(left, _)| left);
    let parts: Vec<_> = core.split('.').collect();
    if parts.len() != 3
        || !parts
            .iter()
            .all(|part| !part.is_empty() && part.chars().all(|ch| ch.is_ascii_digit()))
    {
        return Err(CartError::Invalid(format!(
            "{label} must be semver x.y.z: {value}"
        )));
    }
    Ok(())
}

fn validate_version_constraint(value: &str) -> CartResult<()> {
    if let Some(version) = value.strip_prefix(">=") {
        validate_semver("version constraint", version)
    } else if let Some(version) = value.strip_prefix('^') {
        validate_semver("version constraint", version)
    } else {
        validate_semver("version constraint", value)
    }
}

fn validate_market_index(index: &CartMarketIndex) -> CartResult<()> {
    if index.schema != CART_MARKET_INDEX_SCHEMA {
        return Err(CartError::Invalid(format!(
            "cart market index schema must be {CART_MARKET_INDEX_SCHEMA}, got {}",
            index.schema
        )));
    }
    if index.source.id.trim().is_empty() {
        return Err(CartError::Invalid(
            "market source id is required".to_string(),
        ));
    }
    for cart in &index.carts {
        validate_reverse_dns_id("cart id", &cart.cart_id)?;
        for release in &cart.releases {
            validate_semver("cart release", &release.cart_release)?;
            for artifact in &release.artifacts {
                if artifact.format != CART_FORMAT {
                    return Err(CartError::Invalid(format!(
                        "artifact format must be {CART_FORMAT}, got {}",
                        artifact.format
                    )));
                }
                for resource in &artifact.resources {
                    validate_reverse_dns_id("resource id", &resource.resource_id)?;
                    validate_semver("resource version", &resource.version)?;
                }
            }
        }
    }
    Ok(())
}

fn trust_allows(requester: ResourceTrustClass, candidate: ResourceTrustClass) -> bool {
    requester != ResourceTrustClass::Official || candidate == ResourceTrustClass::Official
}

fn version_satisfies(version: &str, constraint: &str) -> CartResult<bool> {
    validate_semver("resource version", version)?;
    validate_version_constraint(constraint)?;

    if let Some(minimum) = constraint.strip_prefix(">=") {
        Ok(parse_semver_core(version)? >= parse_semver_core(minimum)?)
    } else if let Some(base) = constraint.strip_prefix('^') {
        let version = parse_semver_core(version)?;
        let base = parse_semver_core(base)?;
        Ok(version >= base && version < base.caret_upper_bound())
    } else {
        Ok(parse_semver_core(version)? == parse_semver_core(constraint)?)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct SemverCore {
    major: u64,
    minor: u64,
    patch: u64,
}

impl SemverCore {
    fn caret_upper_bound(self) -> Self {
        if self.major > 0 {
            Self {
                major: self.major + 1,
                minor: 0,
                patch: 0,
            }
        } else if self.minor > 0 {
            Self {
                major: 0,
                minor: self.minor + 1,
                patch: 0,
            }
        } else {
            Self {
                major: 0,
                minor: 0,
                patch: self.patch + 1,
            }
        }
    }
}

fn parse_semver_core(value: &str) -> CartResult<SemverCore> {
    let core = value.split_once('-').map_or(value, |(left, _)| left);
    let core = core.split_once('+').map_or(core, |(left, _)| left);
    let mut parts = core.split('.');
    let major = parse_semver_part(value, parts.next())?;
    let minor = parse_semver_part(value, parts.next())?;
    let patch = parse_semver_part(value, parts.next())?;
    if parts.next().is_some() {
        return Err(CartError::Invalid(format!(
            "semver core must have exactly three parts: {value}"
        )));
    }
    Ok(SemverCore {
        major,
        minor,
        patch,
    })
}

fn parse_semver_part(original: &str, value: Option<&str>) -> CartResult<u64> {
    let value = value.ok_or_else(|| {
        CartError::Invalid(format!(
            "semver core must have exactly three parts: {original}"
        ))
    })?;
    value.parse::<u64>().map_err(|error| {
        CartError::Invalid(format!(
            "invalid semver numeric part in {original}: {error}"
        ))
    })
}

fn sha256_file(path: &Path) -> CartResult<String> {
    let mut file = File::open(path).map_err(|source| CartError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    sha256_reader(path, &mut file)
}

fn sha256_reader(path: &Path, reader: &mut impl Read) -> CartResult<String> {
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer).map_err(|source| CartError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("sha256:{}", hex_lower(&hasher.finalize())))
}

fn hex_lower(bytes: &[u8]) -> String {
    const TABLE: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(TABLE[(byte >> 4) as usize] as char);
        output.push(TABLE[(byte & 0x0f) as usize] as char);
    }
    output
}

fn to_zip_name(path: &Path) -> CartResult<String> {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => parts.push(part.to_string_lossy().to_string()),
            _ => {
                return Err(CartError::Invalid(format!(
                    "unsafe relative path: {}",
                    path.display()
                )))
            }
        }
    }
    let name = parts.join("/");
    validate_relative_entry_name(&name)?;
    Ok(name)
}

fn to_platform_path(name: &str) -> PathBuf {
    name.split('/').collect()
}

fn artifact_code(cart: &Cartfile) -> String {
    let mut code = cart
        .id
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_uppercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    code.push('-');
    code.push_str(&cart.version.replace('.', "-"));
    code
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame_script_fixture() -> ResourceManifest {
        toml::from_str(
            r#"
schema = "continue.resource.v1"
type = "script"
id = "org.example.script.overlay"
name = "Overlay"
version = "1.0.0"
author = "Example"
license = "MIT"
description = "Fixture"
entrypoint = "src/main.js"
[frame_script]
permissions = ["draw", "audio", "read_memory"]
sounds = ["audio/click.wav"]
[frame_script.images]
portrait = "images/portrait.png"
"#,
        )
        .unwrap()
    }

    #[test]
    fn frame_script_manifest_requires_explicit_valid_closed_assets() {
        let manifest = frame_script_fixture();
        validate_resource_manifest(&manifest).unwrap();
        assert_eq!(
            ResourceType::from_str("script").unwrap(),
            ResourceType::Script
        );
        let entries = [
            "resource/continue.toml",
            "resource/src/main.js",
            "resource/audio/click.wav",
            "resource/images/portrait.png",
        ]
        .map(str::to_owned);
        validate_bundle_entrypoints("resource/continue.toml", &manifest, &entries).unwrap();
        assert!(
            validate_bundle_entrypoints("resource/continue.toml", &manifest, &entries[..3])
                .is_err()
        );

        for invalid in [
            "../escape.png",
            "images//escape.png",
            "images/C:escape.png",
            "/images/a.png",
            "images/a.wav",
        ] {
            let mut candidate = frame_script_fixture();
            candidate
                .frame_script
                .as_mut()
                .unwrap()
                .images
                .insert("portrait".into(), invalid.into());
            assert!(validate_resource_manifest(&candidate).is_err(), "{invalid}");
        }
        let mut candidate = frame_script_fixture();
        candidate
            .frame_script
            .as_mut()
            .unwrap()
            .permissions
            .retain(|p| *p != FrameScriptPermission::Audio);
        assert!(validate_resource_manifest(&candidate).is_err());
        let mut candidate = frame_script_fixture();
        candidate
            .frame_script
            .as_mut()
            .unwrap()
            .music
            .push("audio/click.wav".into());
        assert!(
            validate_resource_manifest(&candidate).is_err(),
            "同路径不能同时声明音效和音乐"
        );
        let mut candidate = frame_script_fixture();
        candidate
            .frame_script
            .as_mut()
            .unwrap()
            .permissions
            .push(FrameScriptPermission::WriteMemory);
        candidate
            .frame_script
            .as_mut()
            .unwrap()
            .permissions
            .retain(|p| *p != FrameScriptPermission::ReadMemory);
        assert!(validate_resource_manifest(&candidate).is_err());
        let mut candidate = frame_script_fixture();
        candidate.entrypoint = Some("downloaded.cdsb".into());
        assert!(validate_resource_manifest(&candidate).is_err());
        let mut candidate = frame_script_fixture();
        candidate.resource_type = ResourceType::Diorama;
        assert!(validate_resource_manifest(&candidate).is_err());
    }

    #[test]
    fn local_presentation_checks_identity_and_hashes_files() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/carts/com.example.arcade-demo");
        let temp = tempfile::tempdir().unwrap();
        let bundle = temp.path().join("demo.cart");
        let packed = pack_source_tree(&root, &bundle).unwrap();
        let details = read_bundle_presentation(&bundle, &packed.bundle_sha256).unwrap();
        assert_eq!(details.report.cart.id, "com.example.arcade-demo");
        let file = details
            .files
            .iter()
            .find(|f| f.path == "cartfile.toml")
            .unwrap();
        assert_eq!(
            file.sha256,
            format!(
                "{:x}",
                Sha256::digest(fs::read(root.join("cartfile.toml")).unwrap())
            )
        );
        assert!(read_bundle_presentation(&bundle, &"0".repeat(64)).is_err());
    }

    fn plugin_manifest(runtime: &str, contribution: &str) -> ResourceManifest {
        toml::from_str(&format!(
            r#"schema = "continue.resource.v1"
type = "plugin"
id = "dev.krtek.typed-plugin"
name = "Typed Plugin"
version = "0.1.0"
author = "Krtek"
license = "UNLICENSED"
description = "fixture"
entrypoint = "dist/extension.js"
[plugin]
commands = ["fixture.search", "fixture.scrape"]
[plugin.runtime]
{runtime}
{contribution}
"#
        ))
        .expect("typed plugin fixture必须可解析")
    }

    #[test]
    fn validates_the_complete_typed_plugin_runtime_contract() {
        let valid = plugin_manifest(
            "kind = \"javascript\"\ncontract = \"continue-js-1\"\nhost_api = \"0.1\"",
            "[[plugin.contributes.scrapers]]\nid = \"fixture\"\nsearch_command = \"fixture.search\"\nscrape_command = \"fixture.scrape\"",
        );
        validate_resource_manifest(&valid).expect("valid JavaScript plugin必须通过");
        // ADR 0136：标准 metadata 入口与刮削器声明。
        let standard = plugin_manifest(
            "kind = \"javascript\"\ncontract = \"continue-js-1\"\nhost_api = \"0.1\"",
            "[[plugin.contributes.scrapers]]\nid = \"fixture\"\nmetadata_command = \"fixture.scrape\"\nsupported_settings = [\"maxCandidates\"]\nsources = [\"igdb\"]\nlanguages = [\"en\"]\nlookup = [\"identifier\", \"name\"]\naccepts_identifiers = [\"igdb:game\"]\nemits_identifiers = [\"igdb:game\"]\nprovides = [\"metadata\", \"cover\"]",
        );
        validate_resource_manifest(&standard).expect("metadata_command scraper必须通过");
        let missing_metadata = plugin_manifest(
            "kind = \"javascript\"\ncontract = \"continue-js-1\"\nhost_api = \"0.1\"",
            "[[plugin.contributes.scrapers]]\nid = \"fixture\"\nmetadata_command = \"missing\"",
        );
        assert!(matches!(
            validate_resource_manifest(&missing_metadata),
            Err(CartError::Invalid(_))
        ));
        let mut old_schema = valid.clone();
        old_schema.schema = "krtek.krtekfile.v1".to_owned();
        assert!(matches!(
            validate_resource_manifest(&old_schema),
            Err(CartError::Invalid(_))
        ));
        let mut missing_runtime = valid.clone();
        missing_runtime
            .plugin
            .as_mut()
            .expect("fixture plugin必须存在")
            .runtime = None;
        assert!(matches!(
            validate_resource_manifest(&missing_runtime),
            Err(CartError::Invalid(_))
        ));

        for runtime in [
            "kind = \"javascript\"\ncontract = \"krtek-js-1\"\nhost_api = \"0.1\"",
            "kind = \"javascript\"\nabi = \"continue-wasm-1\"\ncontract = \"continue-js-1\"\nhost_api = \"0.1\"",
            "kind = \"wasm\"\ncontract = \"continue-js-1\"\nhost_api = \"0.1\"",
            "kind = \"javascript\"\ncontract = \"continue-js-1\"\nhost_api = \"999.0\"",
        ] {
            let invalid = plugin_manifest(runtime, "");
            assert!(matches!(
                validate_resource_manifest(&invalid),
                Err(CartError::Invalid(_))
            ));
        }

        let invalid_contribution = plugin_manifest(
            "kind = \"javascript\"\ncontract = \"continue-js-1\"\nhost_api = \"0.1\"",
            "[[plugin.contributes.scrapers]]\nid = \"fixture\"\nsearch_command = \"missing.search\"\nscrape_command = \"fixture.scrape\"",
        );
        assert!(matches!(
            validate_resource_manifest(&invalid_contribution),
            Err(CartError::Invalid(_))
        ));

        let mut duplicate_permissions = valid;
        let permissions = &mut duplicate_permissions
            .plugin
            .as_mut()
            .expect("fixture plugin必须存在")
            .permissions;
        permissions.capabilities = vec!["net.request".to_owned(), "net.request".to_owned()];
        permissions.filesystem = vec![PluginFilesystemPermissionManifest {
            mount: "extension-cache".to_owned(),
            access: vec!["read".to_owned(), "read".to_owned()],
        }];
        assert!(matches!(
            validate_resource_manifest(&duplicate_permissions),
            Err(CartError::Invalid(_))
        ));
    }

    #[test]
    fn catalog_cover_requires_valid_pixel_stream_and_bounded_dimensions() {
        let image = image::DynamicImage::new_rgb8(2, 2);
        for (format, content_type) in [
            (image::ImageFormat::Png, "image/png"),
            (image::ImageFormat::Jpeg, "image/jpeg"),
            (image::ImageFormat::WebP, "image/webp"),
        ] {
            let mut encoded = io::Cursor::new(Vec::new());
            image.write_to(&mut encoded, format).unwrap();
            assert!(validate_catalog_cover_image(encoded.get_ref(), Some(content_type)).is_ok());
        }
        let mut encoded = io::Cursor::new(Vec::new());
        image
            .write_to(&mut encoded, image::ImageFormat::Png)
            .unwrap();
        let mut broken = encoded.into_inner();
        assert!(validate_catalog_cover_image(&broken, Some("image/jpeg")).is_err());
        let mut offset = 8;
        while offset + 12 <= broken.len() {
            let size = u32::from_be_bytes(broken[offset..offset + 4].try_into().unwrap()) as usize;
            if &broken[offset + 4..offset + 8] == b"IDAT" {
                broken[offset + 8] = 0xff; // 破坏 zlib 像素流，同时重算 CRC，保留合法 PNG chunk 框架。
                let mut crc = 0xffff_ffff_u32;
                for byte in &broken[offset + 4..offset + 8 + size] {
                    crc ^= u32::from(*byte);
                    for _ in 0..8 {
                        crc = (crc >> 1) ^ (0xedb8_8320_u32 & 0_u32.wrapping_sub(crc & 1));
                    }
                }
                broken[offset + 8 + size..offset + 12 + size]
                    .copy_from_slice(&(!crc).to_be_bytes());
                break;
            }
            offset += size + 12;
        }
        assert!(validate_catalog_cover_image(&broken, Some("image/png")).is_err());
        let wide = image::DynamicImage::new_rgb8(8193, 1);
        let mut encoded = io::Cursor::new(Vec::new());
        wide.write_to(&mut encoded, image::ImageFormat::Png)
            .unwrap();
        assert!(validate_catalog_cover_image(encoded.get_ref(), Some("image/png")).is_err());
    }

    #[test]
    fn system_catalog_preparation_requires_zip_manifest_and_usable_datasets() {
        let source = tempfile::tempdir().unwrap();
        fs::create_dir(source.path().join("system")).unwrap();
        fs::write(source.path().join("cartfile.toml"), "schema=\"continue.cartfile.v1\"\nid=\"org.continue.system.catalog\"\nname=\"Catalog\"\nversion=\"1.0.0\"\nauthor=\"Continue\"\npublisher=\"Continue\"\ndescription=\"test\"\n[artifact]\nformat=\"cart.zip.v1\"\n[include]\npaths=[\"system/\"]\n").unwrap();
        fs::write(source.path().join("system/continue.toml"), "schema=\"continue.resource.v1\"\ntype=\"system\"\nid=\"org.continue.resource.system.catalog\"\nname=\"Catalog\"\nversion=\"1.0.0\"\nauthor=\"Continue\"\nlicense=\"CC0-1.0\"\ndescription=\"test\"\nentrypoint=\"catalog.json\"\n").unwrap();
        let mut document = serde_json::json!({"schema":"continue.system.catalog.v1","version":"1.0.0","datasets":{
            "genres":[{"id":"genre.rpg","name":"RPG"}],"gameplay":[],"platforms":[],"companies":[],"series":[]
        }});
        let output = tempfile::tempdir().unwrap();
        for (name, valid) in [
            ("good", true),
            ("bad-schema", false),
            ("empty-genres", false),
        ] {
            if name == "bad-schema" {
                document["schema"] = "wrong".into();
            }
            if name == "empty-genres" {
                document["schema"] = "continue.system.catalog.v1".into();
                document["datasets"]["genres"] = serde_json::json!([]);
            }
            fs::write(
                source.path().join("system/catalog.json"),
                document.to_string(),
            )
            .unwrap();
            let bundle = output.path().join(format!("{name}.cart"));
            pack_source_tree(source.path(), &bundle).unwrap();
            let sha = sha256_file(&bundle).unwrap();
            assert_eq!(verify_system_catalog_bundle(&bundle, &sha).is_ok(), valid);
        }
        document["schema"] = "continue.system.catalog.v1".into();
        document["datasets"]["genres"] = serde_json::json!([{"id":"genre.rpg","name":"RPG"}]);
        fs::write(
            source.path().join("system/catalog.json"),
            document.to_string(),
        )
        .unwrap();
        let original_manifest =
            fs::read_to_string(source.path().join("system/continue.toml")).unwrap();
        for (name, dependency, requirement, candidate_version, valid) in [
            (
                "missing-dependency",
                "org.continue.resource.system.missing",
                ">=1.0.0",
                "1.0.0",
                false,
            ),
            (
                "wrong-dependency-version",
                "org.continue.resource.system.catalog",
                ">=2.0.0",
                "1.0.0",
                false,
            ),
            (
                "matching-dependency-version",
                "org.continue.resource.system.catalog",
                "^1.0.0",
                "1.0.0",
                true,
            ),
            (
                "prerelease-does-not-meet-stable",
                "org.continue.resource.system.catalog",
                ">=1.0.0",
                "1.0.0-alpha",
                false,
            ),
        ] {
            document["version"] = candidate_version.into();
            fs::write(
                source.path().join("system/catalog.json"),
                document.to_string(),
            )
            .unwrap();
            let manifest = original_manifest.replace(
                "version=\"1.0.0\"",
                &format!("version=\"{candidate_version}\""),
            );
            fs::write(source.path().join("system/continue.toml"), format!("{manifest}\n[[requires]]\ntype=\"system\"\nid=\"{dependency}\"\nversion=\"{requirement}\"\n")).unwrap();
            let bundle = output.path().join(format!("{name}.cart"));
            pack_source_tree(source.path(), &bundle).unwrap();
            let sha = sha256_file(&bundle).unwrap();
            assert_eq!(verify_system_catalog_bundle(&bundle, &sha).is_ok(), valid);
        }
        let invalid = output.path().join("not-zip.cart");
        fs::write(&invalid, b"this is not a cart archive").unwrap();
        assert!(verify_system_catalog_bundle(&invalid, &sha256_file(&invalid).unwrap()).is_err());
    }

    #[test]
    fn system_resources_round_trip_and_validate_entrypoints() {
        let source = tempfile::tempdir().unwrap();
        fs::write(
            source.path().join("cartfile.toml"),
            r#"schema = "continue.cartfile.v1"
id = "org.continue.system.test"
name = "系统字典"
version = "0.1.0"
author = "Krtek"
publisher = "Krtek"
description = "test"
[artifact]
format = "cart.zip.v1"
[include]
paths = ["system/"]
"#,
        )
        .unwrap();
        fs::create_dir(source.path().join("system")).unwrap();
        fs::write(
            source.path().join("system/continue.toml"),
            r#"schema = "continue.resource.v1"
type = "system"
id = "org.continue.resource.system.test"
name = "基础字典"
version = "0.1.0"
author = "Krtek"
license = "UNLICENSED"
description = "test"
entrypoint = "catalog.json"
"#,
        )
        .unwrap();
        let destination = tempfile::tempdir().unwrap();
        let bundle = destination.path().join("system.cart");
        assert!(pack_source_tree(source.path(), &bundle).is_err());
        fs::write(source.path().join("system/catalog.json"), "{}").unwrap();
        fs::create_dir_all(source.path().join("system/nested")).unwrap();
        fs::write(source.path().join("system/nested/continue.toml"), "schema=\"continue.resource.v1\"\ntype=\"system\"\nid=\"org.continue.resource.system.nested\"\nname=\"Nested\"\nversion=\"0.1.0\"\nauthor=\"Continue\"\nlicense=\"CC0-1.0\"\ndescription=\"test\"\nentrypoint=\"data.json\"\n").unwrap();
        fs::write(source.path().join("system/nested/data.json"), "{}").unwrap();
        pack_source_tree(source.path(), &bundle).unwrap();
        let report = verify_bundle(&bundle).unwrap();
        assert_eq!(report.resources[0].resource_type, ResourceType::System);
        assert_eq!(
            read_system_resource(
                &bundle,
                "org.continue.resource.system.test",
                &report.bundle_sha256,
                32
            )
            .unwrap(),
            b"{}"
        );
        assert!(read_system_resource(
            &bundle,
            "org.continue.resource.system.test",
            &report.bundle_sha256,
            0
        )
        .is_err());
        assert!(read_system_resource(
            &bundle,
            "org.continue.resource.system.test",
            &"0".repeat(64),
            32
        )
        .is_err());
        assert_eq!(
            read_system_resource_member(
                &bundle,
                "org.continue.resource.system.test",
                "catalog.json",
                &report.bundle_sha256,
                32
            )
            .unwrap(),
            b"{}"
        );
        assert!(read_system_resource_member(
            &bundle,
            "org.continue.resource.system.test",
            "nested/data.json",
            &report.bundle_sha256,
            32
        )
        .is_err());
        assert_eq!(
            read_system_resource_member(
                &bundle,
                "org.continue.resource.system.nested",
                "data.json",
                &report.bundle_sha256,
                32
            )
            .unwrap(),
            b"{}"
        );
        for member in [
            "../catalog.json",
            "/etc/passwd",
            "https://example.test/cover.png",
            "C:/cover.png",
            "folder\\cover.png",
        ] {
            assert!(read_system_resource_member(
                &bundle,
                "org.continue.resource.system.test",
                member,
                &report.bundle_sha256,
                32
            )
            .is_err());
        }
        assert!(read_system_resource_member(
            &bundle,
            "org.continue.resource.system.test",
            "catalog.json",
            &report.bundle_sha256,
            1
        )
        .is_err());
        assert_eq!(
            ResourceType::from_str("system").unwrap(),
            ResourceType::System
        );
        let index = generate_market_index(
            std::slice::from_ref(&bundle),
            CartMarketSource {
                id: "test.system".into(),
                name: "test".into(),
                trust_class: ResourceTrustClass::Other,
            },
        )
        .unwrap();
        assert!(serde_json::to_string(&index)
            .unwrap()
            .contains("\"system\""));
        let second = destination.path().join("second.cart");
        pack_source_tree(source.path(), &second).unwrap();
        assert_eq!(fs::read(&bundle).unwrap(), fs::read(second).unwrap());

        fs::remove_dir_all(source.path().join("system/nested")).unwrap();
        let current = source.path().join("system/continue.toml");
        let legacy = source.path().join("system/krtekfile.toml");
        fs::rename(&current, &legacy).unwrap();
        pack_source_tree(source.path(), &bundle).unwrap();
        assert!(verify_bundle(&bundle).unwrap().resources.is_empty());
        fs::copy(&legacy, &current).unwrap();
        pack_source_tree(source.path(), &bundle).unwrap();
        assert_eq!(verify_bundle(&bundle).unwrap().resources.len(), 1);
        let entries = [
            "continue.toml".into(),
            "system/continue.toml".into(),
            "system/krtekfile.toml".into(),
            "system/notcontinue.toml".into(),
        ];
        assert_eq!(
            resource_manifest_entries(&entries).collect::<Vec<_>>(),
            vec!["continue.toml", "system/continue.toml"]
        );
    }

    #[test]
    fn packs_and_verifies_example_cart() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/carts/org.continue.core.mgba-macos-aarch64");
        let temp_dir = tempfile::tempdir().unwrap();
        let output = temp_dir.path().join("mgba.cart");

        let pack = pack_source_tree(&root, &output).unwrap();
        assert!(pack.bundle_sha256.starts_with("sha256:"));
        assert!(pack.entries.iter().any(|entry| entry == "cartfile.toml"));

        let verify = verify_bundle(&output).unwrap();
        assert_eq!(verify.cart.id, "org.continue.core.mgba");
        assert_eq!(verify.resources.len(), 1);
        assert_eq!(verify.resources[0].resource_type, ResourceType::Core);
    }

    #[test]
    fn distribution_bundle_verifies_real_cart_and_preserves_digest() {
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/carts/org.continue.core.mgba-macos-aarch64");
        let root = tempfile::tempdir().unwrap();
        let bundle = root.path().join("example.cart");
        let packed = pack_source_tree(&source, &bundle).unwrap();
        let report = verify_distribution_bundle(&bundle).unwrap();
        assert_eq!(report.bundle_sha256, packed.bundle_sha256);
        assert_eq!(report.path, bundle);
        assert_eq!(report.cart.id, "org.continue.core.mgba");
        assert_eq!(report.resources.len(), 1);
    }

    #[test]
    fn distribution_bundle_rejects_bundle_entry_expanded_and_manifest_limits() {
        use std::io::Write;

        let root = tempfile::tempdir().unwrap();
        let bundle = root.path().join("limit.cart");
        File::create(&bundle)
            .unwrap()
            .set_len(256 * 1024 * 1024 + 1)
            .unwrap();
        let error = verify_distribution_bundle(&bundle).unwrap_err();
        assert!(error.to_string().contains("bundle size"), "{error}");

        let mut zip = ZipWriter::new(io::Cursor::new(Vec::new()));
        for index in 0..8193 {
            zip.start_file(format!("{index}.txt"), SimpleFileOptions::default())
                .unwrap();
        }
        fs::write(&bundle, zip.finish().unwrap().into_inner()).unwrap();
        let error = verify_distribution_bundle(&bundle).unwrap_err();
        assert!(error.to_string().contains("too many"), "{error}");

        // 直接构造不可信 central directory 尺寸，确认在解压及分配大内存前拒绝。
        let mut zip = ZipWriter::new(io::Cursor::new(Vec::new()));
        zip.start_file("cartfile.toml", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"schema=\"continue.cartfile.v1\"").unwrap();
        let original = zip.finish().unwrap().into_inner();
        let central = original
            .windows(4)
            .position(|n| n == b"PK\x01\x02")
            .unwrap();
        for (size, expected) in [
            (1024_u32 * 1024 * 1024 + 1, "expanded size"),
            (1024 * 1024 + 1, "manifest"),
        ] {
            let mut bytes = original.clone();
            bytes[central + 24..central + 28].copy_from_slice(&size.to_le_bytes());
            fs::write(&bundle, bytes).unwrap();
            let error = verify_distribution_bundle(&bundle).unwrap_err();
            assert!(error.to_string().contains(expected), "{error}");
        }
    }

    #[test]
    fn bundle_verification_rejects_a_missing_runtime_entrypoint() {
        let source = tempfile::tempdir().unwrap();
        fs::write(
            source.path().join("cartfile.toml"),
            r#"schema = "continue.cartfile.v1"
id = "dev.krtek.missing-entrypoint"
name = "Missing Entrypoint"
version = "0.1.0"
author = "Krtek"
publisher = "Krtek"
description = "fixture"
[artifact]
format = "cart.zip.v1"
[include]
paths = ["continue.toml"]
"#,
        )
        .unwrap();
        fs::write(
            source.path().join("continue.toml"),
            r#"schema = "continue.resource.v1"
type = "plugin"
id = "dev.krtek.missing-entrypoint"
name = "Missing Entrypoint"
version = "0.1.0"
author = "Krtek"
license = "UNLICENSED"
description = "fixture"
entrypoint = "dist/extension.js"
[plugin]
commands = ["fixture.invoke"]
[plugin.runtime]
kind = "javascript"
contract = "continue-js-1"
host_api = "0.1"
"#,
        )
        .unwrap();
        let output = source.path().join("missing.cart");
        let error = pack_source_tree(source.path(), &output)
            .expect_err("source inspection/pack must reject a missing entrypoint");
        assert!(matches!(error, CartError::Invalid(message) if message.contains("entrypoint")));
    }

    #[test]
    fn packs_and_verifies_the_quickjs_fixture_entrypoint() {
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../extensions/fixtures/igdb-like-js");
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("igdb-like.cart");

        let pack = pack_source_tree(&source, &output).unwrap();
        assert!(pack
            .entries
            .iter()
            .any(|entry| entry == "dist/extension.js"));
        let verified = verify_bundle(&output).unwrap();
        assert_eq!(verified.resources.len(), 1);
        assert_eq!(verified.resources[0].resource_type, ResourceType::Plugin);
    }

    #[test]
    fn generates_market_index_from_source_tree() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/carts/com.starvingartist.atomic-purple-pack");
        let index = generate_market_index(
            &[root],
            CartMarketSource {
                id: "local.test".to_string(),
                name: "Local Test".to_string(),
                trust_class: ResourceTrustClass::Other,
            },
        )
        .unwrap();

        let resources = &index.carts[0].releases[0].artifacts[0].resources;
        assert!(resources
            .iter()
            .any(|resource| resource.resource_type == ResourceType::Diorama));
        assert!(resources
            .iter()
            .any(|resource| resource.resource_type == ResourceType::Preset));
    }

    #[test]
    fn resolves_resource_from_market_index() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/carts/com.starvingartist.atomic-purple-pack");
        let index = generate_market_index(
            &[root],
            CartMarketSource {
                id: "local.test".to_string(),
                name: "Local Test".to_string(),
                trust_class: ResourceTrustClass::Other,
            },
        )
        .unwrap();

        let report = resolve_resources(
            &[index],
            &[ResourceResolveRequest {
                resource_type: ResourceType::Diorama,
                resource_id: "com.starvingartist.diorama.handheld-purple".to_string(),
                version: "^1.0.0".to_string(),
                requester_trust_class: ResourceTrustClass::Other,
            }],
        )
        .unwrap();

        assert_eq!(report.requests[0].state, ResourceResolutionState::Resolved);
        assert_eq!(
            report.requests[0].selected.as_ref().unwrap().cart_id,
            "com.starvingartist.atomic-purple-pack"
        );
    }

    #[test]
    fn official_request_does_not_default_to_other_candidate() {
        let index = CartMarketIndex {
            schema: CART_MARKET_INDEX_SCHEMA.to_string(),
            generated_at: "2026-07-27T00:00:00Z".to_string(),
            source: CartMarketSource {
                id: "local.test".to_string(),
                name: "Local Test".to_string(),
                trust_class: ResourceTrustClass::Other,
            },
            carts: vec![CartIndexEntry {
                cart_id: "com.example.cart".to_string(),
                cart_name: "Example Cart".to_string(),
                description: String::new(),
                publisher: "Example".to_string(),
                releases: vec![CartReleaseIndexEntry {
                    cart_release: "1.0.0".to_string(),
                    artifacts: vec![CartArtifactIndexEntry {
                        cart_artifact: "EXAMPLE-1-0-0".to_string(),
                        format: CART_FORMAT.to_string(),
                        artifact_target: BTreeMap::new(),
                        bundle_sha256: format!("sha256:{}", "0".repeat(64)),
                        signature: BTreeMap::new(),
                        official_trust_evidence: BTreeMap::new(),
                        download: DownloadEntry {
                            kind: "local_file".to_string(),
                            url: "/tmp/example.cart".to_string(),
                        },
                        resources: vec![ResourceDiscoveryProjection {
                            resource_type: ResourceType::Core,
                            resource_id: "com.example.core".to_string(),
                            name: "Example Core".to_string(),
                            version: "1.0.0".to_string(),
                            resource_path: "cores/example/".to_string(),
                            trust_class: ResourceTrustClass::Other,
                            requires: Vec::new(),
                        }],
                    }],
                }],
            }],
        };

        let report = resolve_resources(
            &[index],
            &[ResourceResolveRequest {
                resource_type: ResourceType::Core,
                resource_id: "com.example.core".to_string(),
                version: ">=1.0.0".to_string(),
                requester_trust_class: ResourceTrustClass::Official,
            }],
        )
        .unwrap();

        assert_eq!(
            report.requests[0].state,
            ResourceResolutionState::Unresolved
        );
        assert!(report.requests[0].selected.is_none());
        assert_eq!(report.requests[0].candidates.len(), 1);
    }
}
