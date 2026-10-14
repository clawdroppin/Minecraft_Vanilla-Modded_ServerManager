use crate::error::{AppError, AppResult};
use crate::state::TaskReporter;
use futures_util::StreamExt;
use std::path::Path;
use tokio::io::AsyncWriteExt;

/// Stream a URL to disk, reporting progress. Writes to `<dest>.part` and renames on success.
pub async fn download(
    http: &reqwest::Client,
    url: &str,
    dest: &Path,
    reporter: Option<&TaskReporter>,
    label: &str,
) -> AppResult<()> {
    if let Some(p) = dest.parent() {
        tokio::fs::create_dir_all(p).await?;
    }
    let resp = http.get(url).send().await?.error_for_status()?;
    let total = resp.content_length();
    let part = dest.with_file_name(format!("{}.part", dest.file_name().unwrap_or_default().to_string_lossy()));
    let mut file = tokio::fs::File::create(&part).await?;
    let mut stream = resp.bytes_stream();
    let mut got: u64 = 0;
    let mut last_emit = std::time::Instant::now();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk).await?;
        got += chunk.len() as u64;
        if let Some(r) = reporter {
            if last_emit.elapsed().as_millis() > 120 {
                last_emit = std::time::Instant::now();
                let pct = total.map(|t| got as f64 / t.max(1) as f64);
                r.progress(
                    &format!("Downloading {label} – {:.1} MB", got as f64 / 1_048_576.0),
                    pct,
                );
            }
        }
    }
    file.flush().await?;
    drop(file);
    if got == 0 {
        return Err(AppError::msg(format!("Downloaded file from {url} was empty")));
    }
    if dest.exists() {
        let _ = tokio::fs::remove_file(dest).await;
    }
    tokio::fs::rename(&part, dest).await?;
    Ok(())
}

/// Extract a zip on a blocking thread, refusing entries that would escape `dest`.
pub async fn extract_zip(zip_path: &Path, dest: &Path) -> AppResult<()> {
    let zip_path = zip_path.to_path_buf();
    let dest = dest.to_path_buf();
    tokio::task::spawn_blocking(move || -> AppResult<()> {
        let f = std::fs::File::open(&zip_path)?;
        let mut archive = zip::ZipArchive::new(f)?;
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)?;
            let Some(rel) = entry.enclosed_name() else { continue };
            let out = dest.join(rel);
            if entry.is_dir() {
                std::fs::create_dir_all(&out)?;
            } else {
                if let Some(p) = out.parent() {
                    std::fs::create_dir_all(p)?;
                }
                let mut w = std::fs::File::create(&out)?;
                std::io::copy(&mut entry, &mut w)?;
            }
        }
        Ok(())
    })
    .await
    .map_err(|e| AppError::msg(e.to_string()))?
}

/// SHA-1 of a file (hex) – Modrinth identifies files by it, Mojang publishes it for server jars.
pub fn sha1_file(path: &Path) -> AppResult<String> {
    use std::io::Read;
    let mut f = std::fs::File::open(path)?;
    let mut h = sha1_smol::Sha1::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.digest().to_string())
}

pub fn sha256_file(path: &Path) -> AppResult<String> {
    use sha2::Digest;
    use std::io::Read;
    let mut f = std::fs::File::open(path)?;
    let mut h = sha2::Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

pub async fn verify_sha256(path: &Path, expected: &str) -> AppResult<bool> {
    let p = path.to_path_buf();
    let got = tokio::task::spawn_blocking(move || sha256_file(&p)).await.map_err(|e| AppError::msg(e.to_string()))??;
    Ok(got.eq_ignore_ascii_case(expected.trim()))
}

pub async fn verify_sha1(path: &Path, expected: &str) -> AppResult<bool> {
    let p = path.to_path_buf();
    let got = tokio::task::spawn_blocking(move || sha1_file(&p)).await.map_err(|e| AppError::msg(e.to_string()))??;
    Ok(got.eq_ignore_ascii_case(expected.trim()))
}

/// `download_verified` with up to three attempts – one dropped connection shouldn't fail a
/// modpack of 100+ files.
pub async fn download_retry(http: &reqwest::Client, url: &str, dest: &Path, sha1: Option<&str>, sha256: Option<&str>, label: &str) -> AppResult<()> {
    let mut last = None;
    for attempt in 0..3u64 {
        if attempt > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(800 * attempt)).await;
        }
        match download_verified(http, url, dest, sha1, sha256, None, label).await {
            Ok(()) => return Ok(()),
            Err(e) => last = Some(e),
        }
    }
    Err(AppError::msg(format!("{label}: {}", last.map(|e| e.to_string()).unwrap_or_default())))
}

/// Download and check a hash when one is known; a mismatch deletes the file and fails.
pub async fn download_verified(
    http: &reqwest::Client,
    url: &str,
    dest: &Path,
    sha1: Option<&str>,
    sha256: Option<&str>,
    reporter: Option<&TaskReporter>,
    label: &str,
) -> AppResult<()> {
    download(http, url, dest, reporter, label).await?;
    let ok = match (sha1, sha256) {
        (_, Some(h)) => verify_sha256(dest, h).await?,
        (Some(h), None) => verify_sha1(dest, h).await?,
        _ => true,
    };
    if !ok {
        let _ = tokio::fs::remove_file(dest).await;
        return Err(AppError::msg(format!("{label} failed its checksum – the download was corrupted. Try again.")));
    }
    Ok(())
}
