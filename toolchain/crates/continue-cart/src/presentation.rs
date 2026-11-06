//! 只读取已冻结的本机制品；展示字段不构成信任或执行授权。
use super::*;

pub struct BundlePresentation {
    pub report: VerifyReport,
    pub files: Vec<BundleFile>,
    pub pages: Vec<BundlePage>,
    pub cover: Option<(String, Vec<u8>)>,
    pub byte_size: u64,
    pub resource_manifests: Vec<ResourceManifest>,
}
pub struct BundleFile {
    pub path: String,
    pub byte_size: u64,
    pub sha256: String,
}
pub struct BundlePage {
    pub language: String,
    pub kind: String,
    pub content: String,
}

pub fn read_bundle_presentation(
    path: &Path,
    expected_sha256: &str,
) -> CartResult<BundlePresentation> {
    let io_error = |source| CartError::Io {
        path: path.into(),
        source,
    };
    let mut source = File::open(path).map_err(io_error)?;
    let byte_size = source.metadata().map_err(io_error)?.len();
    if byte_size > 256 * 1024 * 1024 {
        return Err(CartError::Invalid("Cart too large".into()));
    }
    let mut frozen = tempfile::NamedTempFile::new().map_err(io_error)?;
    let copied = io::copy(
        &mut source.by_ref().take(256 * 1024 * 1024 + 1),
        &mut frozen,
    )
    .map_err(io_error)?;
    if copied != byte_size || copied > 256 * 1024 * 1024 {
        return Err(CartError::Invalid("Cart changed".into()));
    }
    let report = verify_distribution_bundle(frozen.path())?;
    if report
        .bundle_sha256
        .strip_prefix("sha256:")
        .unwrap_or(&report.bundle_sha256)
        != expected_sha256
            .strip_prefix("sha256:")
            .unwrap_or(expected_sha256)
    {
        return Err(CartError::Invalid("Cart checksum mismatch".into()));
    }
    let mut archive =
        ZipArchive::new(File::open(frozen.path()).map_err(io_error)?).map_err(|source| {
            CartError::Zip {
                path: path.into(),
                source,
            }
        })?;
    if archive.len() > 10000 {
        return Err(CartError::Invalid("too many Cart files".into()));
    }
    let mut files = Vec::new();
    let mut expanded = 0u64;
    let mut page_bytes = 0usize;
    let mut pages = Vec::new();
    let mut cover = None;
    let mut resource_manifests = Vec::new();
    for index in 0..archive.len() {
        let mut file = archive.by_index(index).map_err(|source| CartError::Zip {
            path: path.into(),
            source,
        })?;
        if file.is_dir() {
            continue;
        }
        let name = file.name().to_owned();
        let mut digest = Sha256::new();
        let page = page_identity(&name);
        let is_manifest = resource_manifest_root(&name).is_some();
        let cover_type = if report.cart.presentation.get("cover") == Some(&name) {
            match Path::new(&name).extension().and_then(|s| s.to_str()) {
                Some("png") => Some("image/png"),
                Some("jpg" | "jpeg") => Some("image/jpeg"),
                Some("webp") => Some("image/webp"),
                _ => None,
            }
        } else {
            None
        };
        let retain = page.is_some() && file.size() <= 128 * 1024
            || cover_type.is_some() && file.size() <= 8 * 1024 * 1024
            || is_manifest && file.size() <= 1024 * 1024;
        let mut bytes = Vec::new();
        let mut buffer = [0u8; 16384];
        let mut actual = 0u64;
        loop {
            let read = file.read(&mut buffer).map_err(io_error)?;
            if read == 0 {
                break;
            }
            actual += read as u64;
            expanded += read as u64;
            if actual > file.size() || expanded > 1024 * 1024 * 1024 {
                return Err(CartError::Invalid("Cart expanded size mismatch".into()));
            }
            digest.update(&buffer[..read]);
            if retain {
                bytes.extend_from_slice(&buffer[..read]);
            }
        }
        if actual != file.size() {
            return Err(CartError::Invalid("Cart expanded size mismatch".into()));
        }
        files.push(BundleFile {
            path: name,
            byte_size: file.size(),
            sha256: format!("{:x}", digest.finalize()),
        });
        if retain {
            if is_manifest {
                let text = std::str::from_utf8(&bytes)
                    .map_err(|_| CartError::Invalid("resource manifest is not UTF-8".into()))?;
                resource_manifests.push(toml::from_str(text).map_err(|source| {
                    CartError::Toml {
                        path: files
                            .last()
                            .map(|f| PathBuf::from(&f.path))
                            .unwrap_or_default(),
                        source,
                    }
                })?);
            }
            if let Some((language, kind)) = page {
                if pages.len() < 16 && page_bytes + bytes.len() <= 2 * 1024 * 1024 {
                    if let Ok(content) = String::from_utf8(bytes.clone()) {
                        page_bytes += content.len();
                        pages.push(BundlePage {
                            language,
                            kind,
                            content,
                        });
                    }
                }
            }
            if let Some(kind) = cover_type {
                if validate_catalog_cover_image(&bytes, Some(kind)).is_ok() {
                    cover = Some((kind.into(), bytes));
                }
            }
        }
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    pages.sort_by(|a, b| {
        a.language
            .cmp(&b.language)
            .then_with(|| b.kind.cmp(&a.kind))
    });
    pages.dedup_by(|a, b| a.language == b.language);
    Ok(BundlePresentation {
        report,
        files,
        pages,
        cover,
        byte_size,
        resource_manifests,
    })
}

// 根目录的 README[.语言].md / index[.语言].html 是静态介绍；不解析包内任意路径。
fn page_identity(name: &str) -> Option<(String, String)> {
    if name.contains('/') {
        return None;
    }
    let (stem, kind) = name
        .strip_suffix(".md")
        .map(|s| (s, "md"))
        .or_else(|| name.strip_suffix(".html").map(|s| (s, "html")))?;
    let prefix = if kind == "md" { "README" } else { "index" };
    let language = if stem == prefix {
        "und"
    } else {
        stem.strip_prefix(&format!("{prefix}."))?
    };
    if language.is_empty()
        || language.len() > 35
        || !language
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return None;
    }
    Some((language.into(), kind.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actual_read_limit_rejects_underreported_zip_sizes() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("cartfile.toml"), "schema=\"continue.cartfile.v1\"\nid=\"org.example.pages\"\nname=\"Pages\"\nversion=\"1.0.0\"\nauthor=\"Author\"\npublisher=\"Author\"\ndescription=\"test\"\n[artifact]\nformat=\"cart.zip.v1\"\n[include]\npaths=[\"README.md\"]\n").unwrap();
        fs::write(source.join("README.md"), "x".repeat(256 * 1024)).unwrap();
        let bundle = root.path().join("pages.cart");
        pack_source_tree(&source, &bundle).unwrap();
        let mut bytes = fs::read(&bundle).unwrap();
        let mut index = 0;
        while index + 46 < bytes.len() {
            if bytes[index..].starts_with(b"PK\x01\x02") {
                let length = u16::from_le_bytes([bytes[index + 28], bytes[index + 29]]) as usize;
                if bytes[index + 46..].starts_with(b"README.md") {
                    bytes[index + 24..index + 28].copy_from_slice(&1u32.to_le_bytes());
                    break;
                }
                index += 46 + length;
            } else {
                index += 1;
            }
        }
        fs::write(&bundle, &bytes).unwrap();
        let sha = format!("{:x}", Sha256::digest(&bytes));
        assert!(
            matches!(read_bundle_presentation(&bundle, &sha), Err(CartError::Invalid(message)) if message == "Cart expanded size mismatch")
        );
    }
}
