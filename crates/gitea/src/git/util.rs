use crate::types::GitError;
use git2::{CertificateCheckStatus, Cred, RemoteCallbacks};

/// 确认远程地址为 SSH 方案，否则 deploy key 无法认证。
pub fn ensure_ssh_url(remote_url: &str) -> Result<(), GitError> {
    let lowered = remote_url.to_ascii_lowercase();
    if remote_url.starts_with("git@") || lowered.starts_with("ssh://") {
        Ok(())
    } else {
        Err(GitError::InvalidPath(format!(
            "使用deploy key 认证仅支持 SSH 地址（git@host:repo.git / ssh://host/...）。请在仓库页面选择 SSH 地址后填入 REMOTE_URL，当前值：{remote_url}"
        )))
    }
}

/// 统一配置 SSH 凭证回调。
pub fn configure_ssh_callbacks_with_key(
    callbacks: &mut RemoteCallbacks<'_>,
    private_key: String,
    passphrase: Option<&str>,
) {
    let passphrase_for_cb = passphrase.map(|s| s.to_string());

    callbacks.credentials(move |_url, username_from_url, _allowed_types| {
        let username = username_from_url.unwrap_or("git");
        Cred::ssh_key_from_memory(username, None, &private_key, passphrase_for_cb.as_deref())
    });

    // 演示场景中直接信任服务器返回的 host key，避免手动维护 known_hosts。
    callbacks.certificate_check(|_cert, _hostname| Ok(CertificateCheckStatus::CertificateOk));
}

pub fn configure_ssh_callbacks_with_password(
    callbacks: &mut RemoteCallbacks<'_>,
    user_name: String,
    password: String,
) {
    callbacks.credentials(move |_url, username_from_url, _allowed_types| {
        let username = username_from_url.unwrap_or(&user_name);
        Cred::userpass_plaintext(username, password.as_str())
    });

    // 演示场景中直接信任服务器返回的 host key，避免手动维护 known_hosts。
    callbacks.certificate_check(|_cert, _hostname| Ok(CertificateCheckStatus::CertificateOk));
}
