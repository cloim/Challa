//! Run with a new isolated profile: cargo run -p challa-bitwarden --example status_probe -- <profile-dir>
//! Prints connection state only. Does not read a default CLI profile or credentials.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let profile = std::env::args_os()
        .nth(1)
        .ok_or("an isolated profile directory is required")?;
    let client = challa_bitwarden::BitwardenClient::with_profile(profile.into());
    let info = client.inspect()?;
    println!("{}", serde_json::to_string(&info)?);
    #[cfg(windows)]
    println!(
        "windows_session_locked={}",
        challa_bitwarden::windows_session_locked()
    );
    Ok(())
}
