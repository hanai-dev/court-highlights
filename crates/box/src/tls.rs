//! The staff console's own certificate (A-124): generated once on first start, kept in the data directory, so its
//! fingerprint stays the same for the staff guide. Browsers show a one-time warning; HSTS is deliberately not sent.

use std::path::Path;

pub fn load_or_create(dir: &Path) -> std::io::Result<(String, String)> {
    let (cert_path, key_path) = (dir.join("staff-cert.pem"), dir.join("staff-key.pem"));
    if cert_path.exists() && key_path.exists() {
        return Ok((std::fs::read_to_string(cert_path)?, std::fs::read_to_string(key_path)?));
    }
    let host = std::env::var("CH_BOX_NAME").unwrap_or_else(|_| "court-box".into());
    let names = vec!["localhost".to_string(), host.clone(), format!("{host}.local")];
    let generated = rcgen::generate_simple_self_signed(names).map_err(std::io::Error::other)?;
    let (cert, key) = (generated.cert.pem(), generated.signing_key.serialize_pem());
    std::fs::write(&cert_path, &cert)?;
    write_private(&key_path, &key)?;
    Ok((cert, key))
}

#[cfg(unix)]
fn write_private(path: &Path, contents: &str) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(path)?.write_all(contents.as_bytes())
}

#[cfg(not(unix))]
fn write_private(path: &Path, contents: &str) -> std::io::Result<()> {
    std::fs::write(path, contents)
}
