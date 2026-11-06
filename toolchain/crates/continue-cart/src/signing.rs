//! Cart 索引的现行 Ed25519 信封；信任根必须由调用者独立配置。

use crate::{CartError, CartResult};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

const SCHEMA: &str = "continue.cart-index-signature.v1";
const ALGORITHM: &str = "Ed25519";
const PUBLIC_KEY_DER_PREFIX: &[u8] = &[
    0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
];

/// 签名覆盖原始 payload bytes，公钥只能来自调用方配置的信任根。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedIndexEnvelope {
    pub schema: String,
    pub algorithm: String,
    pub key_id: String,
    pub payload: String,
    pub signature: String,
}

/// 按现行索引信封签名；不解析或重新序列化 payload。
pub fn sign_index(payload: &[u8], private_key: &[u8; 32]) -> CartResult<SignedIndexEnvelope> {
    let key = SigningKey::from_bytes(private_key);
    Ok(SignedIndexEnvelope {
        schema: SCHEMA.into(),
        algorithm: ALGORITHM.into(),
        key_id: key_id(&public_key_der(private_key)),
        payload: STANDARD.encode(payload),
        signature: STANDARD.encode(key.sign(payload).to_bytes()),
    })
}

/// 仅验证签名并返回精确 bytes；sequence、来源权限和撤回语义仍由索引使用者验证。
pub fn verify_index(
    envelope: &SignedIndexEnvelope,
    trusted_keys: &BTreeMap<String, String>,
) -> CartResult<Vec<u8>> {
    if envelope.schema != SCHEMA || envelope.algorithm != ALGORITHM {
        return Err(CartError::Invalid(
            "unsupported index signature schema or algorithm".into(),
        ));
    }
    let trusted_der = trusted_keys
        .get(&envelope.key_id)
        .ok_or_else(|| CartError::Invalid("untrusted index signing key".into()))?;
    let der = STANDARD
        .decode(trusted_der)
        .map_err(|_| CartError::Invalid("invalid index signing key encoding".into()))?;
    if der.len() != PUBLIC_KEY_DER_PREFIX.len() + 32
        || !der.starts_with(PUBLIC_KEY_DER_PREFIX)
        || envelope.key_id != key_id(&der)
    {
        return Err(CartError::Invalid(
            "invalid index signing key DER or key_id".into(),
        ));
    }
    let raw = der[PUBLIC_KEY_DER_PREFIX.len()..]
        .try_into()
        .map_err(|_| CartError::Invalid("invalid index signing key length".into()))?;
    let key = VerifyingKey::from_bytes(raw)
        .map_err(|_| CartError::Invalid("invalid index signing key".into()))?;
    let payload = STANDARD
        .decode(&envelope.payload)
        .map_err(|_| CartError::Invalid("invalid signed index payload encoding".into()))?;
    let signature = STANDARD
        .decode(&envelope.signature)
        .map_err(|_| CartError::Invalid("invalid index signature encoding".into()))?;
    let signature = Signature::from_slice(&signature)
        .map_err(|_| CartError::Invalid("invalid index signature length".into()))?;
    key.verify_strict(&payload, &signature)
        .map_err(|_| CartError::Invalid("index signature verification failed".into()))?;
    Ok(payload)
}

/// 返回与 Depot OpenSSL public_to_der 相同的 Ed25519 SPKI DER。
pub fn public_key_der(private_key: &[u8; 32]) -> Vec<u8> {
    let mut der = PUBLIC_KEY_DER_PREFIX.to_vec();
    der.extend_from_slice(
        SigningKey::from_bytes(private_key)
            .verifying_key()
            .as_bytes(),
    );
    der
}

/// 已有 key_id 契约固定为完整公钥 DER 的 SHA-256。
pub fn key_id(der: &[u8]) -> String {
    format!("ed25519:{:x}", Sha256::digest(der))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRIVATE_KEY: [u8; 32] = [7; 32];
    const PUBLIC_DER: &str = "MCowBQYDK2VwAyEA6kpsY+KcUgq+9VB7Ey7F+ZVHdq6+vnuSQh7qaRRG0iw=";
    const KEY_ID: &str = "ed25519:324be2dea8bc44461b0233e51fa48902ed6b1cc671e7739af2551e0bfe68f54e";
    const PAYLOAD: &[u8] =
        b"{\n  \"schema\": \"continue.cart-market-index.v1\", \"sequence\": 1, \"carts\": []\n}\n";

    // 使用 Depot 同款 OpenSSL Ed25519 / public_to_der 生成，固定原始空白以检查 bytes 互通。
    fn openssl_envelope() -> SignedIndexEnvelope {
        serde_json::from_value(serde_json::json!({
            "schema": "continue.cart-index-signature.v1",
            "algorithm": "Ed25519",
            "key_id": KEY_ID,
            "payload": "ewogICJzY2hlbWEiOiAiY29udGludWUuY2FydC1tYXJrZXQtaW5kZXgudjEiLCAic2VxdWVuY2UiOiAxLCAiY2FydHMiOiBbXQp9Cg==",
            "signature": "EcCirXxnztq4jw7rFzyzxYP5ONI1G49+OOScs9F0yYNhrm/vp1DHxIS32SN61kmT3hzlr2YPDEhHaEF+3wn8BA=="
        }))
        .unwrap()
    }

    fn trusted_keys() -> BTreeMap<String, String> {
        BTreeMap::from([(KEY_ID.into(), PUBLIC_DER.into())])
    }

    #[test]
    fn verifies_existing_openssl_envelope_without_reserializing_payload() {
        assert_eq!(
            verify_index(&openssl_envelope(), &trusted_keys()).unwrap(),
            PAYLOAD
        );
    }

    #[test]
    fn signing_matches_openssl_signature_der_and_key_id() {
        let der = public_key_der(&PRIVATE_KEY);
        assert_eq!(STANDARD.encode(&der), PUBLIC_DER);
        assert_eq!(key_id(&der), KEY_ID);
        let signed = sign_index(PAYLOAD, &PRIVATE_KEY).unwrap();
        assert_eq!(
            serde_json::to_value(signed).unwrap(),
            serde_json::to_value(openssl_envelope()).unwrap()
        );
    }

    #[test]
    fn round_trip_preserves_all_original_bytes() {
        let bytes = b"\x00\xff exact bytes\n";
        let signed = sign_index(bytes, &PRIVATE_KEY).unwrap();
        assert_eq!(verify_index(&signed, &trusted_keys()).unwrap(), bytes);
    }

    #[test]
    fn rejects_tampered_payload_signature_and_envelope_contract() {
        let original = openssl_envelope();
        for field in ["payload", "signature", "schema", "algorithm"] {
            let mut signed = serde_json::to_value(&original).unwrap();
            signed[field] = match field {
                "payload" => STANDARD.encode(b"changed payload").into(),
                "signature" => STANDARD.encode([0; 64]).into(),
                _ => "invalid".into(),
            };
            let signed = serde_json::from_value(signed).unwrap();
            assert!(verify_index(&signed, &trusted_keys()).is_err(), "{field}");
        }
    }

    #[test]
    fn rejects_unknown_key_invalid_der_and_mismatched_key_id() {
        let signed = openssl_envelope();
        assert!(verify_index(&signed, &BTreeMap::new()).is_err());
        let mut invalid_der = STANDARD.decode(PUBLIC_DER).unwrap();
        invalid_der[0] ^= 1;
        for der in [
            "invalid base64".to_owned(),
            STANDARD.encode([0; 32]),
            STANDARD.encode(invalid_der),
            STANDARD.encode(public_key_der(&[8; 32])),
        ] {
            let keys = BTreeMap::from([(KEY_ID.into(), der)]);
            assert!(verify_index(&signed, &keys).is_err());
        }
    }

    #[test]
    fn rejects_malformed_base64_signature_length_and_unknown_envelope_fields() {
        let original = openssl_envelope();
        for field in ["payload", "signature"] {
            let mut value = serde_json::to_value(&original).unwrap();
            value[field] = "not base64".into();
            let signed = serde_json::from_value(value).unwrap();
            assert!(verify_index(&signed, &trusted_keys()).is_err());
        }
        let mut signed = original.clone();
        signed.signature = STANDARD.encode([0; 63]);
        assert!(verify_index(&signed, &trusted_keys()).is_err());
        let mut value = serde_json::to_value(original).unwrap();
        value["public_key"] = PUBLIC_DER.into();
        assert!(serde_json::from_value::<SignedIndexEnvelope>(value).is_err());
    }
}
