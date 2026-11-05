//! Snippet files are encrypted for the current Windows account with DPAPI.
use challa_core::Snippet;
use std::path::Path;
use windows::Win32::Foundation::{LocalFree, HLOCAL};
use windows::Win32::Security::Cryptography::*;
use zeroize::Zeroize;

fn protect(bytes: &[u8], encrypt: bool) -> Result<Vec<u8>, String> {
    let input = CRYPT_INTEGER_BLOB {
        cbData: bytes.len() as u32,
        pbData: bytes.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    unsafe {
        let result = if encrypt {
            CryptProtectData(
                &input,
                None,
                None,
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &input,
                None,
                None,
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        result.map_err(|error| format!("스니펫 암호화 저장소: {error}"))?;
        let bytes = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        // The DPAPI buffer also contains plaintext on decrypt.
        std::slice::from_raw_parts_mut(output.pbData, output.cbData as usize).zeroize();
        let _ = LocalFree(Some(HLOCAL(output.pbData as *mut _)));
        Ok(bytes)
    }
}
pub fn load_value<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Option<T>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let ciphertext = std::fs::read(path).map_err(|error| error.to_string())?;
    let plaintext = zeroize::Zeroizing::new(protect(&ciphertext, false)?);
    serde_json::from_slice(&plaintext)
        .map(Some)
        .map_err(|error| format!("저장된 스니펫을 확인해 주세요: {error}"))
}
pub fn save_value<T: serde::Serialize + ?Sized>(path: &Path, snippets: &T) -> Result<(), String> {
    let plaintext =
        zeroize::Zeroizing::new(serde_json::to_vec(snippets).map_err(|e| e.to_string())?);
    let ciphertext = protect(&plaintext, true)?;
    let parent = path.parent().ok_or("저장소 경로를 확인해 주세요")?;
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let temp = path.with_extension("pending");
    std::fs::write(&temp, ciphertext).map_err(|error| error.to_string())?;
    // MoveFileEx replaces atomically on Windows. Keep the old file if saving fails.
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };
    let from: Vec<u16> = temp.as_os_str().encode_wide().chain(Some(0)).collect();
    let to: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    unsafe {
        MoveFileExW(
            PCWSTR(from.as_ptr()),
            PCWSTR(to.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    }
    .map_err(|error| error.to_string())
}
pub fn load(path: &Path) -> Result<Option<Vec<Snippet>>, String> {
    load_value(path)
}
pub fn save(path: &Path, snippets: &[Snippet]) -> Result<(), String> {
    save_value(path, snippets)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn encrypted_store_roundtrip_preserves_disabled_and_unicode_content() {
        let path =
            std::env::temp_dir().join(format!("challa-storage-test-{}.dat", std::process::id()));
        let mut snippet = Snippet::plain_text("1", "서명", ":sig", "감사합니다.\nChalla 드림 😀");
        snippet.enabled = false;
        save(&path, &[snippet.clone()]).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert!(!bytes.windows(6).any(|part| part == b"Challa"));
        assert_eq!(load(&path).unwrap().unwrap(), vec![snippet]);
        std::fs::remove_file(path).unwrap();
    }
}
