pub mod client;
pub mod error;
pub mod models;
#[cfg(windows)]
mod profile;

pub use client::BitwardenClient;
pub use error::BitwardenError;
pub use models::{
    BitwardenField, SecretFieldValue, VaultInfo, VaultItemSummary, VaultReference, VaultStatus,
};

/// Query only the current session's lock flag. Unknown state is treated as locked.
#[cfg(windows)]
pub fn windows_session_locked() -> bool {
    use windows::{core::PWSTR, Win32::System::RemoteDesktop::*};
    unsafe {
        let mut buffer = PWSTR::null();
        let mut length = 0;
        if WTSQuerySessionInformationW(
            None,
            WTS_CURRENT_SESSION,
            WTSSessionInfoEx,
            &mut buffer,
            &mut length,
        )
        .is_err()
        {
            return true;
        }
        let locked = if !buffer.is_null() && length as usize >= std::mem::size_of::<WTSINFOEXW>() {
            let info = &*buffer.0.cast::<WTSINFOEXW>();
            info.Level != 1
                || info.Data.WTSInfoExLevel1.SessionState != WTSActive
                || info.Data.WTSInfoExLevel1.SessionFlags != WTS_SESSIONSTATE_UNLOCK as i32
        } else {
            true
        };
        WTSFreeMemory(buffer.0.cast());
        locked
    }
}
