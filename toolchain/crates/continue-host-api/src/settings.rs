//! 扩展声明式设置契约。普通值、凭据与运行时授权各自独立。

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// 读取当前调用固定的普通设置快照。
pub const GET_METHOD: &str = "settings.get";
/// 一个扩展的普通设置，不包含凭据。
pub type SettingsValues = BTreeMap<String, Value>;

/// 第一版支持的设置类型；选择框由字符串的 `enum` 推导。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingType {
    Boolean,
    String,
    Integer,
    Number,
    StringList,
    Secret,
    Password,
}

/// 发布 manifest 与宿主共享的设置声明。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingDefinition {
    pub key: String,
    #[serde(rename = "type")]
    pub kind: SettingType,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
    #[serde(default, rename = "enum", skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum: Option<i32>,
}

impl SettingDefinition {
    pub fn is_credential(&self) -> bool {
        matches!(self.kind, SettingType::Secret | SettingType::Password)
    }

    /// 错误只包含稳定字段名，不包含用户输入。
    pub fn validate_value(&self, value: &Value) -> Result<(), String> {
        let valid = match self.kind {
            SettingType::Boolean => value.is_boolean(),
            SettingType::String => value.as_str().is_some_and(|text| {
                text.len() <= 4096
                    && (self.choices.is_empty() || self.choices.iter().any(|v| v == text))
            }),
            SettingType::Integer | SettingType::Number => value.as_f64().is_some_and(|n| {
                n.is_finite()
                    && (self.kind != SettingType::Integer
                        || (n.fract() == 0.0 && n.abs() <= 9_007_199_254_740_991.0))
                    && self.minimum.is_none_or(|min| n >= f64::from(min))
                    && self.maximum.is_none_or(|max| n <= f64::from(max))
            }),
            SettingType::StringList => value.as_array().is_some_and(|values| {
                values.len() <= 64
                    && values
                        .iter()
                        .all(|v| v.as_str().is_some_and(|s| s.len() <= 4096))
            }),
            SettingType::Secret | SettingType::Password => false,
        };
        if valid {
            Ok(())
        } else {
            Err(format!("invalid setting: {}", self.key))
        }
    }
}

/// 在打包和加载时使用相同校验，凭据不允许声明默认值。
pub fn validate_definitions(definitions: &[SettingDefinition]) -> Result<(), String> {
    if definitions.len() > 64 {
        return Err("at most 64 extension settings are supported".into());
    }
    let mut keys = BTreeSet::new();
    for field in definitions {
        let key = field.key.as_bytes();
        if key.is_empty()
            || key.len() > 128
            || !key[0].is_ascii_alphabetic()
            || !key
                .iter()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
            || matches!(
                field.key.as_str(),
                "__proto__" | "constructor" | "prototype"
            )
            || !keys.insert(&field.key)
            || field.title.trim().is_empty()
            || field.title.len() > 256
            || field.description.len() > 4096
        {
            return Err("invalid or duplicate extension setting declaration".into());
        }
        if field.choices.len() > 64
            || field.choices.iter().any(|v| v.len() > 4096)
            || field.choices.iter().collect::<BTreeSet<_>>().len() != field.choices.len()
            || (!field.choices.is_empty() && field.kind != SettingType::String)
            || ((field.minimum.is_some() || field.maximum.is_some())
                && !matches!(field.kind, SettingType::Number | SettingType::Integer))
            || matches!((field.minimum, field.maximum), (Some(min), Some(max)) if min > max)
        {
            return Err(format!("invalid setting constraints: {}", field.key));
        }
        if field.is_credential() {
            if field.default.is_some() {
                return Err("credentials must not declare defaults".into());
            }
        } else {
            field.validate_value(
                field
                    .default
                    .as_ref()
                    .ok_or_else(|| format!("missing setting default: {}", field.key))?,
            )?;
        }
    }
    Ok(())
}

/// 只投影已声明的普通值；旧版或暂未安装扩展的未知键留在配置中，不交给扩展。
pub fn resolve(
    definitions: &[SettingDefinition],
    overrides: &SettingsValues,
) -> Result<SettingsValues, String> {
    let mut values = BTreeMap::new();
    for field in definitions {
        if field.is_credential() {
            if overrides.contains_key(&field.key) {
                return Err("credentials must use secure storage".into());
            }
            continue;
        }
        let value = overrides
            .get(&field.key)
            .or(field.default.as_ref())
            .ok_or_else(|| format!("missing setting: {}", field.key))?;
        field.validate_value(value)?;
        values.insert(field.key.clone(), value.clone());
    }
    Ok(values)
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetRequest {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetResponse {
    pub settings: SettingsValues,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn field(kind: SettingType, default: Option<Value>) -> SettingDefinition {
        SettingDefinition {
            key: "timeout".into(),
            kind,
            title: "超时".into(),
            description: String::new(),
            default,
            choices: vec![],
            minimum: None,
            maximum: None,
        }
    }
    #[test]
    fn validates_declared_defaults_limits_choices_and_credential_separation() {
        let mut number = field(SettingType::Integer, Some(json!(15)));
        number.minimum = Some(1);
        number.maximum = Some(60);
        validate_definitions(&[number.clone()]).unwrap();
        for invalid in [json!(0), json!(61), json!(1.1), json!("15")] {
            assert!(number.validate_value(&invalid).is_err());
        }
        let mut choice = field(SettingType::String, Some(json!("auto")));
        choice.choices = vec!["auto".into(), "manual".into()];
        validate_definitions(&[choice.clone()]).unwrap();
        assert!(choice.validate_value(&json!("unknown")).is_err());
        assert!(validate_definitions(&[choice.clone(), choice]).is_err());
        for kind in [SettingType::Secret, SettingType::Password] {
            validate_definitions(&[field(kind, None)]).unwrap();
            assert!(validate_definitions(&[field(kind, Some(json!("sensitive")))]).is_err());
            assert!(resolve(
                &[field(kind, None)],
                &BTreeMap::from([("timeout".into(), json!("sensitive"))])
            )
            .is_err());
        }
        assert_eq!(
            resolve(
                &[number],
                &BTreeMap::from([("oldUnknownKey".into(), json!(42))])
            )
            .unwrap(),
            BTreeMap::from([("timeout".into(), json!(15))])
        );
    }
}
