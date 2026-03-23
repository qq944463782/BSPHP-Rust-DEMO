//! MD5、AES-128-CBC（IV=key）、RSA PKCS#1 v1.5，与演示站 AppEn 行为一致。

use aes::Aes128;
use base64::Engine;
use cbc::cipher::block_padding::Pkcs7;
use cbc::cipher::{BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use rsa::pkcs8::{DecodePrivateKey, DecodePublicKey};
use rsa::{Pkcs1v15Encrypt, RsaPrivateKey, RsaPublicKey};
use serde_json::Value;

type Aes128CbcEnc = cbc::Encryptor<Aes128>;
type Aes128CbcDec = cbc::Decryptor<Aes128>;

#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    #[error("base64")]
    Base64(#[from] base64::DecodeError),
    #[error("rsa {0}")]
    Rsa(String),
    #[error("json")]
    Json(#[from] serde_json::Error),
    #[error("format")]
    Format,
}

fn strip_pem_wrappers(s: &str) -> String {
    let mut raw = s.trim().to_string();
    for (a, b) in [
        ("-----BEGIN RSA PRIVATE KEY-----", "-----END RSA PRIVATE KEY-----"),
        ("-----BEGIN PRIVATE KEY-----", "-----END PRIVATE KEY-----"),
        ("-----BEGIN PUBLIC KEY-----", "-----END PUBLIC KEY-----"),
    ] {
        raw = raw.replace(a, "").replace(b, "");
    }
    raw.chars().filter(|c| !c.is_whitespace()).collect()
}

pub fn md5_hex(s: &str) -> String {
    use md5::{Digest, Md5};
    let d = Md5::digest(s.as_bytes());
    d.iter().map(|b| format!("{:02x}", b)).collect()
}

fn load_private_key_der_b64(b64: &str) -> Result<RsaPrivateKey, CryptoError> {
    let raw = strip_pem_wrappers(b64);
    let der = base64::engine::general_purpose::STANDARD.decode(raw.as_bytes())?;
    RsaPrivateKey::from_pkcs8_der(&der).map_err(|e| CryptoError::Rsa(e.to_string()))
}

fn load_public_key_der_b64(b64: &str) -> Result<RsaPublicKey, CryptoError> {
    let raw = strip_pem_wrappers(b64);
    let der = base64::engine::general_purpose::STANDARD.decode(raw.as_bytes())?;
    RsaPublicKey::from_public_key_der(&der).map_err(|e| CryptoError::Rsa(e.to_string()))
}

pub fn aes128_cbc_encrypt_base64(plaintext: &str, key16: &str) -> Result<String, CryptoError> {
    let key = key16.as_bytes();
    if key.len() < 16 {
        return Err(CryptoError::Format);
    }
    let key16b: [u8; 16] = key[..16].try_into().map_err(|_| CryptoError::Format)?;
    let iv = key16b;
    let cipher = Aes128CbcEnc::new_from_slices(&key16b, &iv).map_err(|_| CryptoError::Format)?;
    let ct = cipher.encrypt_padded_vec_mut::<Pkcs7>(plaintext.as_bytes());
    Ok(base64::engine::general_purpose::STANDARD.encode(ct))
}

fn aes128_cbc_decrypt_base64(ciphertext_b64: &str, key16: &str) -> Result<String, CryptoError> {
    let key = key16.as_bytes();
    if key.len() < 16 {
        return Err(CryptoError::Format);
    }
    let key16b: [u8; 16] = key[..16].try_into().map_err(|_| CryptoError::Format)?;
    let iv = key16b;
    let mut ct = base64::engine::general_purpose::STANDARD.decode(ciphertext_b64.as_bytes())?;
    let cipher = Aes128CbcDec::new_from_slices(&key16b, &iv).map_err(|_| CryptoError::Format)?;
    let pt = cipher
        .decrypt_padded_vec_mut::<Pkcs7>(&mut ct)
        .map_err(|_| CryptoError::Format)?;
    String::from_utf8(pt).map_err(|_| CryptoError::Format)
}

pub fn rsa_encrypt_pkcs1_base64(
    message: &str,
    public_key_b64_der: &str,
) -> Result<String, CryptoError> {
    let pub_key = load_public_key_der_b64(public_key_b64_der)?;
    let mut rng = rand::thread_rng();
    let enc = pub_key
        .encrypt(&mut rng, Pkcs1v15Encrypt, message.as_bytes())
        .map_err(|e| CryptoError::Rsa(e.to_string()))?;
    Ok(base64::engine::general_purpose::STANDARD.encode(enc))
}

fn rsa_decrypt_pkcs1_base64(
    ciphertext_b64: &str,
    private_key_b64_der: &str,
) -> Result<String, CryptoError> {
    let priv_key = load_private_key_der_b64(private_key_b64_der)?;
    let ct = base64::engine::general_purpose::STANDARD.decode(ciphertext_b64.as_bytes())?;
    let dec = priv_key
        .decrypt(Pkcs1v15Encrypt, &ct)
        .map_err(|e| CryptoError::Rsa(e.to_string()))?;
    String::from_utf8(dec).map_err(|_| CryptoError::Format)
}

/// 解密 HTTP 响应体，返回 `response` 对象（含 `data`、`code`、`appsafecode` 等）。
pub fn decrypt_response_body(
    raw: &str,
    server_private_key_b64: &str,
    appsafecode: &str,
) -> Result<Value, CryptoError> {
    let body = if raw.contains('%') {
        urlencoding::decode(raw)
            .map(|c| c.into_owned())
            .unwrap_or_else(|_| raw.to_string())
    } else {
        raw.to_string()
    };
    let parts: Vec<&str> = body.split('|').collect();
    if parts.len() < 3 {
        return Err(CryptoError::Format);
    }
    let resp_enc_b64 = parts[1].trim();
    let resp_rsa_b64 = parts[2].trim();
    let sig = rsa_decrypt_pkcs1_base64(resp_rsa_b64, server_private_key_b64)?;
    let sig_parts: Vec<&str> = sig.split('|').collect();
    if sig_parts.len() < 4 {
        return Err(CryptoError::Format);
    }
    let key16: String = sig_parts[2].chars().take(16).collect();
    if key16.len() < 16 {
        return Err(CryptoError::Format);
    }
    let decrypted = aes128_cbc_decrypt_base64(resp_enc_b64, &key16)?;
    let j: Value = serde_json::from_str(&decrypted)?;
    let mut resp = j
        .get("response")
        .cloned()
        .filter(|v| v.is_object())
        .ok_or(CryptoError::Format)?;
    if let Some(obj) = resp.as_object_mut() {
        let ok = obj
            .get("appsafecode")
            .and_then(|v| v.as_str())
            .map(|s| s == appsafecode)
            .unwrap_or(false);
        if !ok {
            obj.insert(
                "data".to_string(),
                Value::String("appsafecode 安全参数验证不通过".to_string()),
            );
        }
    }
    Ok(resp)
}

