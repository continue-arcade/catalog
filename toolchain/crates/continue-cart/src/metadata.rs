//! 作者展示元数据；语言选择不会改变 Cart 身份或安装语义。
use super::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LocalizedText {
    Text(String),
    Translations(BTreeMap<String, String>),
}
impl Default for LocalizedText {
    fn default() -> Self {
        Self::Text(String::new())
    }
}
impl LocalizedText {
    pub fn resolve(&self, locale: Option<&str>) -> &str {
        match self {
            Self::Text(text) => text,
            Self::Translations(values) => locale
                .and_then(|key| values.get(key))
                .or_else(|| {
                    locale
                        .and_then(|key| key.split('-').next())
                        .and_then(|key| values.get(key))
                })
                .or_else(|| values.get("en"))
                .or_else(|| values.first_key_value().map(|(_, value)| value))
                .map(String::as_str)
                .unwrap_or(""),
        }
    }
    pub(super) fn validate(&self, max_chars: usize, required: bool) -> CartResult<()> {
        let validate = |value: &str| {
            if (required && value.trim().is_empty())
                || value.chars().count() > max_chars
                || value.contains(['\r', '\n'])
            {
                Err(CartError::Invalid("invalid localized Cart text".into()))
            } else {
                Ok(())
            }
        };
        match self {
            Self::Text(value) => validate(value),
            Self::Translations(values) => {
                if values.is_empty() || values.len() > 32 {
                    return Err(CartError::Invalid("invalid locale map".into()));
                }
                for (locale, value) in values {
                    if !valid_locale(locale) {
                        return Err(CartError::Invalid("invalid locale tag".into()));
                    }
                    validate(value)?;
                }
                Ok(())
            }
        }
    }
}
fn valid_locale(value: &str) -> bool {
    let mut parts = value.split('-');
    let language = parts.next().unwrap_or("");
    (2..=8).contains(&language.len())
        && language.bytes().all(|c| c.is_ascii_alphabetic())
        && parts.all(|part| {
            (1..=8).contains(&part.len()) && part.bytes().all(|c| c.is_ascii_alphanumeric())
        })
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CartPresentation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub screenshots: Vec<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, String>,
}
impl CartPresentation {
    pub fn get(&self, key: &str) -> Option<&String> {
        match key {
            "icon" => self.icon.as_ref(),
            "cover" => self.cover.as_ref(),
            _ => self.extra.get(key),
        }
    }
    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.icon
            .iter()
            .chain(self.cover.iter())
            .chain(self.screenshots.iter())
            .map(String::as_str)
    }
}
impl Cartfile {
    pub fn display_name(&self) -> &str {
        self.name.resolve(self.default_locale.as_deref())
    }
    pub fn display_summary(&self) -> &str {
        self.summary.resolve(self.default_locale.as_deref())
    }
    pub(super) fn validate_metadata(&self) -> CartResult<()> {
        if self.schema == "continue.cartfile.v1" {
            if !matches!(&self.name,LocalizedText::Text(value) if !value.trim().is_empty())
                || !self.summary.resolve(None).is_empty()
                || !self.tags.is_empty()
                || !self.presentation.screenshots.is_empty()
                || self.default_locale.is_some()
            {
                return Err(CartError::Invalid(
                    "new presentation metadata requires continue.cartfile.v2".into(),
                ));
            }
            return Ok(());
        }
        if !self.extra.is_empty() {
            return Err(CartError::Invalid("unknown Cartfile v2 fields".into()));
        }
        self.name.validate(128, true)?;
        self.summary.validate(240, false)?;
        if let Some(locale) = &self.default_locale {
            if !valid_locale(locale) {
                return Err(CartError::Invalid("invalid default_locale".into()));
            }
            if matches!(&self.name,LocalizedText::Translations(values) if !values.contains_key(locale))
            {
                return Err(CartError::Invalid(
                    "default_locale missing from name".into(),
                ));
            }
        }
        let mut tags = BTreeSet::new();
        if self.tags.len() > 32
            || self.tags.iter().any(|tag| {
                tag.is_empty()
                    || tag.len() > 48
                    || !tag
                        .bytes()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
                    || !tags.insert(tag)
            })
        {
            return Err(CartError::Invalid("invalid or duplicate Cart tags".into()));
        }
        if self.presentation.screenshots.len() > 12 {
            return Err(CartError::Invalid("too many screenshots".into()));
        }
        for path in self.presentation.paths() {
            validate_include_path(path)?;
            if !matches!(
                Path::new(path).extension().and_then(|s| s.to_str()),
                Some("png" | "jpg" | "jpeg" | "webp")
            ) {
                return Err(CartError::Invalid(
                    "Cart presentation must use PNG, JPEG or WebP".into(),
                ));
            }
        }
        if self.schema == "continue.cartfile.v1"
            && (!matches!(&self.name, LocalizedText::Text(_))
                || !self.summary.resolve(None).is_empty()
                || !self.tags.is_empty()
                || !self.presentation.screenshots.is_empty()
                || self.default_locale.is_some())
        {
            return Err(CartError::Invalid(
                "localized metadata, summary, tags and screenshots require continue.cartfile.v2"
                    .into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn manifest() -> Cartfile {
        toml::from_str(
            r#"schema="continue.cartfile.v2"
id="org.example.theme"
name={zh-CN="主题",en="Theme"}
summary={zh-CN="简单的主题",en="A simple theme"}
default_locale="zh-CN"
tags=["theme","lcd"]
version="1.0.0"
author="Example"
publisher="Example"
description="Longer description"
[artifact]
format="cart.zip.v1"
[include]
paths=["theme/"]
[presentation]
screenshots=["theme/screenshot.webp"]
"#,
        )
        .unwrap()
    }
    #[test]
    fn localized_metadata_preserves_maps_and_resolves_default() {
        let c = manifest();
        c.validate_metadata().unwrap();
        assert_eq!(c.display_name(), "主题");
        assert_eq!(c.summary.resolve(Some("en")), "A simple theme");
        assert_eq!(serde_json::to_value(&c).unwrap()["name"]["zh-CN"], "主题");
    }
    #[test]
    fn rejects_invalid_locale_tags_duplicates_and_missing_default() {
        let mut c = manifest();
        c.default_locale = Some("fr".into());
        assert!(c.validate_metadata().is_err());
        c.default_locale = None;
        c.tags.push("lcd".into());
        assert!(c.validate_metadata().is_err());
        c.tags.clear();
        c.summary = LocalizedText::Text("x".repeat(241));
        assert!(c.validate_metadata().is_err());
    }
    #[test]
    fn new_metadata_requires_v2_without_rewriting_immutable_v1() {
        let mut c = manifest();
        c.schema = "continue.cartfile.v1".into();
        assert!(c.validate_metadata().is_err());
        c.name = LocalizedText::Text("Theme".into());
        c.summary = Default::default();
        c.tags.clear();
        c.default_locale = None;
        c.presentation.screenshots.clear();
        c.validate_metadata().unwrap();
    }
}
