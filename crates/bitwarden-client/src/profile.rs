//! The official CLI writes authentication tokens to data.json. Keep that profile
//! DPAPI-encrypted between commands and expose a private working copy only while
//! the child process runs. A profile lock prevents concurrent CLI writers.
use crate::BitwardenError as Error;
use std::os::windows::{
    ffi::OsStrExt,
    fs::{MetadataExt, OpenOptionsExt},
};
use std::{
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
};
use windows::{
    core::PCWSTR,
    Win32::{
        Foundation::{LocalFree, HLOCAL},
        Security::Cryptography::*,
        Storage::FileSystem::{MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH},
    },
};
use zeroize::{Zeroize, Zeroizing};

const LIMIT: u64 = 64 * 1024 * 1024;
fn crypt(input: &[u8], encrypt: bool) -> Result<Zeroizing<Vec<u8>>, Error> {
    if input.len() as u64 > LIMIT {
        return Err(Error::InvalidResponse);
    }
    let source = CRYPT_INTEGER_BLOB {
        cbData: input.len() as u32,
        pbData: input.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    unsafe {
        let result = if encrypt {
            CryptProtectData(
                &source,
                None,
                None,
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &source,
                None,
                None,
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        result.map_err(|_| Error::ProfileStorage)?;
        let bytes = Zeroizing::new(
            std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec(),
        );
        std::slice::from_raw_parts_mut(output.pbData, output.cbData as usize).zeroize();
        let _ = LocalFree(Some(HLOCAL(output.pbData.cast())));
        Ok(bytes)
    }
}
fn read_bounded(path: &Path) -> Result<Zeroizing<Vec<u8>>, Error> {
    if std::fs::metadata(path)
        .map_err(|_| Error::InvalidResponse)?
        .len()
        > LIMIT
    {
        return Err(Error::InvalidResponse);
    }
    std::fs::read(path)
        .map(Zeroizing::new)
        .map_err(|_| Error::InvalidResponse)
}
fn clear_working_file(path: &Path) -> Result<(), Error> {
    if !path.exists() {
        return Ok(());
    }
    let metadata = std::fs::symlink_metadata(path).map_err(|_| Error::InvalidResponse)?;
    if metadata.file_attributes() & 0x400 != 0 || !metadata.is_file() {
        return Err(Error::InvalidResponse);
    }
    OpenOptions::new()
        .write(true)
        .open(path)
        .and_then(|file| file.set_len(0))
        .map_err(|_| Error::ProfileStorage)?;
    std::fs::remove_file(path).map_err(|_| Error::ProfileStorage)
}
pub(crate) struct WorkingProfile {
    root: PathBuf,
    active: PathBuf,
    _lock: File,
}
impl WorkingProfile {
    pub fn open(root: &Path) -> Result<Self, Error> {
        std::fs::create_dir_all(root).map_err(|_| Error::ProfileStorage)?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .share_mode(0)
            .open(root.join("profile.lock"))
            .map_err(|_| Error::Busy)?;
        let active = root.join("active");
        std::fs::create_dir_all(&active).map_err(|_| Error::ProfileStorage)?;
        if std::fs::symlink_metadata(&active)
            .map_err(|_| Error::InvalidResponse)?
            .file_attributes()
            & 0x400
            != 0
        {
            return Err(Error::InvalidResponse);
        }
        // A terminated previous process may have left a working file. Recover
        // solely from the last committed encrypted state and discard that copy.
        clear_working_file(&active.join("data.json"))?;
        let guard = Self {
            root: root.to_owned(),
            active,
            _lock: lock,
        };
        let state = root.join("state.dpapi");
        if state.exists() {
            let ciphertext = read_bounded(&state)?;
            let plaintext = crypt(&ciphertext, false)?;
            std::fs::write(guard.active.join("data.json"), &plaintext)
                .map_err(|_| Error::ProfileStorage)?;
        }
        Ok(guard)
    }
    pub fn directory(&self) -> &Path {
        &self.active
    }
    pub fn commit(self) -> Result<(), Error> {
        let working = self.active.join("data.json");
        if working.exists() {
            let plaintext = read_bounded(&working)?;
            // A crashed CLI cannot overwrite the last complete profile.
            serde_json::from_slice::<serde::de::IgnoredAny>(&plaintext)
                .map_err(|_| Error::InvalidResponse)?;
            let ciphertext = crypt(&plaintext, true)?;
            let pending = self.root.join("state.pending");
            std::fs::write(&pending, &ciphertext).map_err(|_| Error::ProfileStorage)?;
            let from: Vec<u16> = pending.as_os_str().encode_wide().chain(Some(0)).collect();
            let state = self.root.join("state.dpapi");
            let to: Vec<u16> = state.as_os_str().encode_wide().chain(Some(0)).collect();
            unsafe {
                MoveFileExW(
                    PCWSTR(from.as_ptr()),
                    PCWSTR(to.as_ptr()),
                    MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
                )
            }
            .map_err(|_| Error::ProfileStorage)?;
        }
        clear_working_file(&working)?;
        Ok(())
    }
}
impl Drop for WorkingProfile {
    fn drop(&mut self) {
        let _ = clear_working_file(&self.active.join("data.json"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn directory(suffix: &str) -> PathBuf {
        std::env::temp_dir().join(format!("challa-bw-profile-{}-{suffix}", std::process::id()))
    }
    fn cleanup(root: &Path) {
        for name in ["state.dpapi", "state.pending", "profile.lock"] {
            let _ = std::fs::remove_file(root.join(name));
        }
        let _ = std::fs::remove_dir(root.join("active"));
        let _ = std::fs::remove_dir(root);
    }
    #[test]
    fn profile_is_encrypted_between_cli_commands_and_cannot_have_two_writers() {
        let root = directory("roundtrip");
        cleanup(&root);
        let profile = WorkingProfile::open(&root).unwrap();
        assert!(matches!(WorkingProfile::open(&root), Err(Error::Busy)));
        let json = br#"{"fixture":"authentication-state"}"#;
        std::fs::write(profile.directory().join("data.json"), json).unwrap();
        profile.commit().unwrap();
        assert!(!root.join("active/data.json").exists());
        let encrypted = std::fs::read(root.join("state.dpapi")).unwrap();
        assert!(!encrypted.windows(7).any(|w| w == b"fixture"));
        let profile = WorkingProfile::open(&root).unwrap();
        assert_eq!(
            &*read_bounded(&profile.directory().join("data.json")).unwrap(),
            json
        );
        drop(profile);
        assert!(!root.join("active/data.json").exists());
        cleanup(&root);
    }
    #[test]
    fn malformed_working_state_keeps_last_committed_profile_and_cleans_plaintext() {
        let root = directory("malformed");
        cleanup(&root);
        let profile = WorkingProfile::open(&root).unwrap();
        std::fs::write(profile.directory().join("data.json"), b"{}").unwrap();
        profile.commit().unwrap();
        let before = std::fs::read(root.join("state.dpapi")).unwrap();
        let profile = WorkingProfile::open(&root).unwrap();
        std::fs::write(profile.directory().join("data.json"), b"broken").unwrap();
        assert!(profile.commit().is_err());
        assert!(!root.join("active/data.json").exists());
        assert_eq!(std::fs::read(root.join("state.dpapi")).unwrap(), before);
        cleanup(&root);
    }
}
