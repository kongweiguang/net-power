//! @author kongweiguang
//! 敏感信息加密服务。SQLite 只保存密文和 nonce，不保存明文密码或 passphrase。

use crate::database::Database;
use crate::error::{AppError, AppResult};
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
#[cfg(not(test))]
use base64::engine::general_purpose::STANDARD;
#[cfg(not(test))]
use base64::Engine;
use rand::RngCore;

#[cfg(not(test))]
const KEYRING_SERVICE: &str = "net-power";
#[cfg(not(test))]
const KEYRING_USER: &str = "master-key";

/// 加密后写入 secrets 表，返回 secret id。
pub fn store_secret(db: &Database, name: &str, kind: &str, plaintext: &str) -> AppResult<String> {
    let key = load_or_create_master_key()?;
    let (ciphertext, nonce) = encrypt_with_key(&key, plaintext)?;
    db.insert_secret(name, kind, &ciphertext, &nonce)
}

/// 从 secrets 表读取并解密明文。
pub fn load_secret(db: &Database, secret_id: &str) -> AppResult<String> {
    let key = load_or_create_master_key()?;
    let (ciphertext, nonce) = db.get_secret(secret_id)?;
    decrypt_with_key(&key, &ciphertext, &nonce)
}

fn encrypt_with_key(key: &[u8; 32], plaintext: &str) -> AppResult<(Vec<u8>, [u8; 12])> {
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|err| AppError::Crypto(format!("初始化 AES 失败: {err}")))?;
    let mut nonce_bytes = [0_u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce_bytes), plaintext.as_bytes())
        .map_err(|err| AppError::Crypto(format!("加密失败: {err}")))?;
    Ok((ciphertext, nonce_bytes))
}

fn decrypt_with_key(key: &[u8; 32], ciphertext: &[u8], nonce: &[u8]) -> AppResult<String> {
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|err| AppError::Crypto(format!("初始化 AES 失败: {err}")))?;
    let plaintext = cipher
        .decrypt(Nonce::from_slice(nonce), ciphertext.as_ref())
        .map_err(|err| AppError::Crypto(format!("解密失败: {err}")))?;
    String::from_utf8(plaintext).map_err(|err| AppError::Crypto(format!("明文不是 UTF-8: {err}")))
}

#[cfg(not(test))]
fn load_or_create_master_key() -> AppResult<[u8; 32]> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)?;
    match entry.get_password() {
        Ok(encoded) => decode_master_key(&encoded),
        Err(keyring::Error::NoEntry) => {
            let mut key = [0_u8; 32];
            rand::thread_rng().fill_bytes(&mut key);
            entry.set_password(&STANDARD.encode(key))?;
            Ok(key)
        }
        Err(err) => Err(AppError::Keyring(err)),
    }
}

#[cfg(test)]
fn load_or_create_master_key() -> AppResult<[u8; 32]> {
    Ok([7_u8; 32])
}

#[cfg(not(test))]
fn decode_master_key(encoded: &str) -> AppResult<[u8; 32]> {
    let decoded = STANDARD
        .decode(encoded)
        .map_err(|err| AppError::Crypto(format!("主密钥不是合法 base64: {err}")))?;
    decoded
        .try_into()
        .map_err(|_| AppError::Crypto("主密钥长度不是 32 字节".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_roundtrip_keeps_plaintext_out_of_sqlite() {
        let db = Database::in_memory().expect("内存数据库应初始化成功");
        let plaintext = "ssh-password-should-not-be-plain";
        let id =
            store_secret(&db, "测试 secret", "ssh_password", plaintext).expect("secret 应加密写入");

        let (ciphertext, nonce) = db.get_secret(&id).expect("secret 密文应可读取");
        assert_ne!(ciphertext, plaintext.as_bytes());
        assert!(!String::from_utf8_lossy(&ciphertext).contains(plaintext));
        assert_eq!(nonce.len(), 12);
        assert_eq!(load_secret(&db, &id).expect("secret 应可解密"), plaintext);
    }
}
