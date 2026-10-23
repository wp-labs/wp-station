//! Station 设备连接 Token 的静态密钥加密。

use std::sync::OnceLock;

use aes_gcm::aead::Aead;
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use rand::RngExt;

const PREFIX: &str = "enc:v1:";
const KEY_ENV: &str = "WP_STATION_DEVICE_TOKEN_KEY";
// 默认密钥使现有部署无需额外配置；它属于程序本身，只保护数据库静态明文。
const DEFAULT_KEY: [u8; 32] = [
    0xf1, 0xfe, 0x1a, 0x72, 0x4e, 0x04, 0x4a, 0xe9, 0x43, 0x69, 0x5d, 0x56, 0x37, 0x2d, 0x9f, 0xd9,
    0xde, 0x0d, 0xb9, 0x04, 0x0b, 0x5c, 0x67, 0xcc, 0xfd, 0x40, 0xe4, 0x7a, 0x30, 0xcb, 0x8e, 0x4d,
];
static KEY: OnceLock<[u8; 32]> = OnceLock::new();

/// 设备 Token 密钥来源。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeySource {
    BuiltIn,
    Environment,
}

impl KeySource {
    /// 用于启动日志，不包含密钥内容。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::BuiltIn => "built_in",
            Self::Environment => "environment",
        }
    }
}

/// 默认使用内置密钥；显式配置时使用 32 字节 Base64 密钥。运行中不允许更换。
pub fn initialize_key() -> Result<KeySource, String> {
    let (key, source) = match std::env::var(KEY_ENV) {
        Ok(encoded) => {
            let decoded = STANDARD
                .decode(encoded.trim())
                .map_err(|_| format!("{KEY_ENV} 必须是 32 字节密钥的 Base64 编码"))?;
            let key: [u8; 32] = decoded
                .try_into()
                .map_err(|_| format!("{KEY_ENV} 解码后必须是 32 字节"))?;
            (key, KeySource::Environment)
        }
        Err(std::env::VarError::NotPresent) => (DEFAULT_KEY, KeySource::BuiltIn),
        Err(std::env::VarError::NotUnicode(_)) => {
            return Err(format!("{KEY_ENV} 不是有效的 UTF-8 文本"));
        }
    };
    if let Some(existing) = KEY.get() {
        if existing != &key {
            return Err("设备 Token 加密密钥在运行中发生变化".to_string());
        }
        return Ok(source);
    }
    KEY.set(key)
        .map_err(|_| "设备 Token 加密密钥初始化失败".to_string())?;
    Ok(source)
}

fn cipher() -> Result<Aes256Gcm, String> {
    let key = KEY.get().ok_or("设备 Token 加密密钥尚未初始化")?;
    Aes256Gcm::new_from_slice(key).map_err(|_| "设备 Token 加密器初始化失败".to_string())
}

/// 每次写入使用独立随机 nonce，密文包含版本及 nonce。
pub fn encrypt(token: &str) -> Result<String, String> {
    if token.is_empty() {
        return Ok(String::new());
    }
    let mut nonce = [0_u8; 12];
    rand::rng().fill(&mut nonce);
    let ciphertext = cipher()?
        .encrypt(Nonce::from_slice(&nonce), token.as_bytes())
        .map_err(|_| "设备 Token 加密失败".to_string())?;
    let mut envelope = nonce.to_vec();
    envelope.extend_from_slice(&ciphertext);
    Ok(format!("{PREFIX}{}", STANDARD.encode(envelope)))
}

/// 只接受已知密文格式；旧明文仅由启动迁移读取。
pub fn decrypt(stored: &str) -> Result<String, String> {
    if stored.is_empty() {
        return Ok(String::new());
    }
    let encoded = stored
        .strip_prefix(PREFIX)
        .ok_or("设备 Token 不是受支持的加密格式")?;
    let envelope = STANDARD
        .decode(encoded)
        .map_err(|_| "设备 Token 密文编码无效")?;
    if envelope.len() < 28 {
        return Err("设备 Token 密文长度无效".to_string());
    }
    let plaintext = cipher()?
        .decrypt(Nonce::from_slice(&envelope[..12]), &envelope[12..])
        .map_err(|_| "设备 Token 解密失败，请检查密钥或数据库内容".to_string())?;
    String::from_utf8(plaintext).map_err(|_| "设备 Token 解密结果不是 UTF-8".to_string())
}

/// 用于启动迁移识别旧明文与已加密记录。
pub fn is_encrypted(stored: &str) -> bool {
    stored.starts_with(PREFIX)
}

/// 拒绝未知版本的密文，避免误把它当明文再次加密。
pub fn is_unknown_envelope(stored: &str) -> bool {
    stored.starts_with("enc:") && !is_encrypted(stored)
}
