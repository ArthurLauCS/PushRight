//! 访问令牌存在 Windows 凭据管理器里，目标名 `PushRight:<主机>`：「控制面板 → 凭据管理器 → Windows 凭据」里看得到、删得掉。

use windows::core::{HSTRING, PWSTR};
use windows::Win32::Security::Credentials::{CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC};

fn target(host: &str) -> String {
    format!("PushRight:{host}")
}

pub fn load(host: &str) -> Option<String> {
    let mut cred: *mut CREDENTIALW = std::ptr::null_mut();
    unsafe {
        CredReadW(&HSTRING::from(target(host)), CRED_TYPE_GENERIC, None, &mut cred).ok()?;
        let c = &*cred;
        let token = String::from_utf8(std::slice::from_raw_parts(c.CredentialBlob, c.CredentialBlobSize as usize).to_vec()).ok();
        CredFree(cred as *const _);
        token
    }
}

pub fn save(host: &str, token: &str) -> Result<(), String> {
    let mut name: Vec<u16> = target(host).encode_utf16().chain([0]).collect();
    let mut user: Vec<u16> = "PushRight".encode_utf16().chain([0]).collect();
    let mut blob = token.as_bytes().to_vec();
    let cred = CREDENTIALW {
        Type: CRED_TYPE_GENERIC,
        TargetName: PWSTR(name.as_mut_ptr()),
        UserName: PWSTR(user.as_mut_ptr()),
        CredentialBlobSize: blob.len() as u32,
        CredentialBlob: blob.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        ..Default::default()
    };
    unsafe { CredWriteW(&cred, 0) }.map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Security::Credentials::CredDeleteW;

    #[test]
    fn saves_and_loads() {
        let host = "pushright-test.invalid";
        save(host, "secret-123").unwrap();
        assert_eq!(load(host).as_deref(), Some("secret-123"));
        unsafe { CredDeleteW(&HSTRING::from(target(host)), CRED_TYPE_GENERIC, None) }.unwrap();
        assert_eq!(load(host), None);
    }
}
