//! osquery provisioning module
//!
//! Downloads and manages osquery binaries from official GitHub releases.

use anyhow::{Context, Result};
use clap::ValueEnum;
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use std::fmt;
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::io::AsyncWriteExt;

/// Host identifier mode for osquery enrollment
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum HostIdentifier {
    /// Use hardware UUID from system_info table (default)
    /// Best for physical machines with unique hardware
    Uuid,
    /// Use osquery's randomly generated instance ID
    /// Best for containers/VMs where hardware UUID may be duplicated
    Instance,
}

impl fmt::Display for HostIdentifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HostIdentifier::Uuid => write!(f, "uuid"),
            HostIdentifier::Instance => write!(f, "instance"),
        }
    }
}

impl HostIdentifier {
    /// Returns the osquery command-line argument value
    pub fn as_osquery_arg(&self) -> &'static str {
        match self {
            HostIdentifier::Uuid => "uuid",
            HostIdentifier::Instance => "instance",
        }
    }
}

/// Current osquery version to download
const OSQUERY_VERSION: &str = "5.20.0";

/// GitHub release URL template
const GITHUB_RELEASE_URL: &str = "https://github.com/osquery/osquery/releases/download";

/// Platform-specific download info
struct PlatformInfo {
    /// Filename to download from GitHub releases
    download_filename: &'static str,
    /// Expected SHA256 hash (from osquery releases)
    sha256: &'static str,
    /// Archive type
    archive_type: ArchiveType,
    /// Path to osqueryd binary within the archive
    binary_path: &'static str,
}

#[derive(Clone, Copy)]
enum ArchiveType {
    TarGz,
    Pkg,    // macOS .pkg (we'll extract manually)
    Zip,    // Windows
}

/// Get platform-specific download info
fn get_platform_info() -> Result<PlatformInfo> {
    // These hashes are from osquery 5.20.0 release
    // https://github.com/osquery/osquery/releases/tag/5.20.0
    
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        Ok(PlatformInfo {
            download_filename: "osquery-5.20.0_1.linux_x86_64.tar.gz",
            sha256: "4f0e4e23c864a72dcb20bf4661ea0d2719358c938ec342105a633cc732dc03c3",
            archive_type: ArchiveType::TarGz,
            binary_path: "opt/osquery/bin/osqueryd",
        })
    }
    
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        Ok(PlatformInfo {
            download_filename: "osquery-5.20.0_1.linux_aarch64.tar.gz",
            sha256: "cb8d942943c765ebd87c5a3b01fc09988c8ad31acf094207fc49e7acf88ec573",
            archive_type: ArchiveType::TarGz,
            binary_path: "opt/osquery/bin/osqueryd",
        })
    }
    
    #[cfg(target_os = "macos")]
    {
        Ok(PlatformInfo {
            download_filename: "osquery-5.20.0.pkg",
            sha256: "569751a8bc4fdd3aba94071a4b840003066b2cff8e1b0ef9abf46c7a482173c0",
            archive_type: ArchiveType::Pkg,
            binary_path: "opt/osquery/lib/osquery.app/Contents/MacOS/osqueryd",
        })
    }
    
    #[cfg(target_os = "windows")]
    {
        Ok(PlatformInfo {
            download_filename: "osquery-5.20.0.windows_x86_64.zip",
            sha256: "af66cb90537c52459539141f183ae8abb3073f29089b5d1f68245381d80967e1",
            archive_type: ArchiveType::Zip,
            binary_path: "osqueryd/osqueryd.exe",
        })
    }
    
    #[cfg(not(any(
        all(target_os = "linux", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "aarch64"),
        target_os = "macos",
        target_os = "windows"
    )))]
    {
        anyhow::bail!("Unsupported platform")
    }
}

/// Manages osquery binary provisioning
pub struct OsqueryProvisioner {
    /// Directory where osquery will be stored
    data_dir: PathBuf,
    /// Skip hash verification (for development)
    skip_verify: bool,
}

impl OsqueryProvisioner {
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            data_dir,
            skip_verify: false,
        }
    }

    /// Allow skipping hash verification (useful during development or when hashes aren't available)
    pub fn skip_verification(mut self, skip: bool) -> Self {
        self.skip_verify = skip;
        self
    }

    /// Get the path where osqueryd should be located
    pub fn osqueryd_path(&self) -> PathBuf {
        #[cfg(target_os = "windows")]
        {
            self.data_dir.join("bin").join("osqueryd.exe")
        }
        #[cfg(target_os = "macos")]
        {
            // On macOS, we keep the .app bundle intact for code signing
            self.data_dir.join("bin").join("osquery.app").join("Contents").join("MacOS").join("osqueryd")
        }
        #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
        {
            self.data_dir.join("bin").join("osqueryd")
        }
    }

    /// Check if osquery is already provisioned
    pub async fn is_provisioned(&self) -> bool {
        let path = self.osqueryd_path();
        if !path.exists() {
            return false;
        }
        
        // Verify it's executable (on Unix)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(metadata) = std::fs::metadata(&path) {
                let perms = metadata.permissions();
                return perms.mode() & 0o111 != 0;
            }
            return false;
        }
        
        #[cfg(not(unix))]
        {
            true
        }
    }

    /// Provision osquery - download if not present
    pub async fn ensure_provisioned(&self) -> Result<PathBuf> {
        if self.is_provisioned().await {
            println!("  osquery:   {} (cached)", self.osqueryd_path().display());
            return Ok(self.osqueryd_path());
        }

        println!("  osquery:   Downloading...");
        self.download_and_extract().await?;
        
        Ok(self.osqueryd_path())
    }

    /// Download osquery from GitHub releases and extract
    async fn download_and_extract(&self) -> Result<()> {
        let platform_info = get_platform_info()?;
        
        let download_url = format!(
            "{}/{}/{}",
            GITHUB_RELEASE_URL, OSQUERY_VERSION, platform_info.download_filename
        );

        println!("             Downloading from GitHub releases...");
        println!("             URL: {}", download_url);

        // Create temp file for download
        let temp_dir = self.data_dir.join("tmp");
        fs::create_dir_all(&temp_dir).await?;
        let temp_file = temp_dir.join(platform_info.download_filename);

        // Download with progress
        self.download_file(&download_url, &temp_file).await?;

        // Verify hash (unless skipped)
        if !self.skip_verify {
            println!("             Verifying checksum...");
            self.verify_hash(&temp_file, platform_info.sha256).await?;
        }

        // Extract based on archive type
        println!("             Extracting...");
        let bin_dir = self.data_dir.join("bin");
        fs::create_dir_all(&bin_dir).await?;

        match platform_info.archive_type {
            ArchiveType::TarGz => {
                self.extract_tar_gz(&temp_file, &bin_dir, platform_info.binary_path).await?;
            }
            ArchiveType::Pkg => {
                self.extract_pkg(&temp_file, &bin_dir, platform_info.binary_path).await?;
            }
            ArchiveType::Zip => {
                self.extract_zip(&temp_file, &bin_dir, platform_info.binary_path).await?;
            }
        }

        // Cleanup temp file
        let _ = fs::remove_file(&temp_file).await;
        let _ = fs::remove_dir(&temp_dir).await;

        // Verify the binary exists and is executable
        let osqueryd_path = self.osqueryd_path();
        if !osqueryd_path.exists() {
            anyhow::bail!("Failed to extract osqueryd binary");
        }

        // Make executable on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&osqueryd_path)?.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&osqueryd_path, perms)?;
        }

        println!("             Done! osqueryd installed at {:?}", osqueryd_path);
        Ok(())
    }

    /// Download a file with progress indication
    async fn download_file(&self, url: &str, dest: &Path) -> Result<()> {
        let client = reqwest::Client::new();
        let response = client
            .get(url)
            .send()
            .await
            .context("Failed to start download")?;

        if !response.status().is_success() {
            anyhow::bail!("Download failed with status: {}", response.status());
        }

        let total_size = response.content_length().unwrap_or(0);
        let mut file = tokio::fs::File::create(dest).await?;
        let mut downloaded: u64 = 0;
        let mut stream = response.bytes_stream();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.context("Error downloading chunk")?;
            file.write_all(&chunk).await?;
            downloaded += chunk.len() as u64;

            // Simple progress indicator
            if total_size > 0 {
                let percent = (downloaded * 100) / total_size;
                print!("\r             Downloaded: {}%   ", percent);
            }
        }
        println!();

        file.flush().await?;
        Ok(())
    }

    /// Verify SHA256 hash of downloaded file
    async fn verify_hash(&self, file: &Path, expected: &str) -> Result<()> {
        let data = fs::read(file).await?;
        let mut hasher = Sha256::new();
        hasher.update(&data);
        let result = hasher.finalize();
        let hash = format!("{:x}", result);

        if hash != expected {
            anyhow::bail!(
                "Hash mismatch!\n  Expected: {}\n  Got: {}",
                expected,
                hash
            );
        }
        Ok(())
    }

    /// Extract osqueryd from a .tar.gz archive
    async fn extract_tar_gz(&self, archive: &Path, dest_dir: &Path, binary_path: &str) -> Result<()> {
        let archive_data = fs::read(archive).await?;
        
        // Decompress and extract in blocking task
        let dest_dir = dest_dir.to_path_buf();
        let binary_path = binary_path.to_string();
        
        tokio::task::spawn_blocking(move || {
            use flate2::read::GzDecoder;
            use std::io::Cursor;
            use tar::Archive;

            let cursor = Cursor::new(archive_data);
            let decoder = GzDecoder::new(cursor);
            let mut archive = Archive::new(decoder);

            for entry in archive.entries()? {
                let mut entry = entry?;
                let path = entry.path()?;
                
                // Check if this is the binary we want
                if path.to_string_lossy().ends_with("osqueryd") || 
                   path.to_string_lossy() == binary_path {
                    let dest_path = dest_dir.join("osqueryd");
                    entry.unpack(&dest_path)?;
                    return Ok(());
                }
            }
            
            anyhow::bail!("osqueryd not found in archive")
        }).await?
    }

    /// Extract osqueryd from a macOS .pkg file
    async fn extract_pkg(&self, pkg_path: &Path, dest_dir: &Path, _binary_path: &str) -> Result<()> {
        // macOS .pkg files are complex - they contain a cpio archive inside
        // We'll use the pkgutil command to expand it
        
        let temp_expand = self.data_dir.join("tmp").join("pkg_expand");
        // Remove any existing temp directory from a previous failed attempt
        // pkgutil --expand-full requires the destination to NOT exist
        let _ = fs::remove_dir_all(&temp_expand).await;

        let output = tokio::process::Command::new("pkgutil")
            .arg("--expand-full")
            .arg(pkg_path)
            .arg(&temp_expand)
            .output()
            .await
            .context("Failed to run pkgutil - is this macOS?")?;

        if !output.status.success() {
            anyhow::bail!(
                "pkgutil failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        // On macOS, we need to copy the entire .app bundle to preserve code signing
        // The app bundle is at: <expanded>/Payload/opt/osquery/lib/osquery.app
        let src_app = temp_expand.join("Payload").join("opt/osquery/lib/osquery.app");
        let dest_app = dest_dir.join("osquery.app");
        
        if !src_app.exists() {
            anyhow::bail!("Could not find osquery.app in pkg at {:?}", src_app);
        }

        // Remove existing app bundle if present
        let _ = fs::remove_dir_all(&dest_app).await;

        // Use cp -R to preserve all attributes and symlinks
        let output = tokio::process::Command::new("cp")
            .arg("-R")
            .arg(&src_app)
            .arg(&dest_app)
            .output()
            .await
            .context("Failed to copy osquery.app bundle")?;

        if !output.status.success() {
            anyhow::bail!(
                "Failed to copy osquery.app: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        // Cleanup
        let _ = fs::remove_dir_all(&temp_expand).await;
        
        Ok(())
    }

    /// Extract osqueryd from a Windows .zip archive
    async fn extract_zip(&self, archive: &Path, dest_dir: &Path, binary_path: &str) -> Result<()> {
        let archive_data = fs::read(archive).await?;
        let dest_dir = dest_dir.to_path_buf();
        let binary_path = binary_path.to_string();

        tokio::task::spawn_blocking(move || {
            use std::io::Cursor;

            let cursor = Cursor::new(archive_data);
            let mut archive = zip::ZipArchive::new(cursor)?;

            for i in 0..archive.len() {
                let mut file = archive.by_index(i)?;
                let name = file.name().to_string();

                // Check if this is osqueryd
                if name.ends_with("osqueryd.exe") || name == binary_path {
                    let dest_path = dest_dir.join("osqueryd.exe");
                    let mut outfile = std::fs::File::create(&dest_path)?;
                    std::io::copy(&mut file, &mut outfile)?;
                    return Ok(());
                }
            }

            anyhow::bail!("osqueryd.exe not found in archive")
        }).await?
    }
}

/// Query osquery for the host identifier based on the selected mode
///
/// - `uuid`: Returns the hardware UUID from `system_info.uuid`
/// - `instance`: Returns the osquery instance ID from `osquery_info.instance_id`
///
/// For `instance` mode, osquery needs a database path to generate/persist the instance ID.
pub async fn get_host_identifier(
    osqueryd_path: &Path,
    mode: &HostIdentifier,
    data_dir: &Path,
) -> Result<String> {
    use std::collections::HashMap;
    use std::process::Stdio;
    use tokio::process::Command;

    let (query, field) = match mode {
        HostIdentifier::Uuid => ("SELECT uuid FROM system_info;", "uuid"),
        HostIdentifier::Instance => ("SELECT instance_id FROM osquery_info;", "instance_id"),
    };

    let mut cmd = Command::new(osqueryd_path);
    cmd.arg("-S"); // Shell mode
    cmd.arg("--json");

    // For instance mode, we need to specify the database path so osquery can
    // generate/retrieve a persistent instance_id
    if *mode == HostIdentifier::Instance {
        cmd.arg("--database_path").arg(data_dir.join("osquery.db"));
    }

    cmd.arg(query);

    let output = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .context("Failed to run osquery")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("osquery query failed: {}", stderr);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse JSON output: [{"uuid": "..."} or {"instance_id": "..."}]
    let parsed: Vec<HashMap<String, String>> =
        serde_json::from_str(&stdout).context("Failed to parse osquery output")?;

    parsed
        .first()
        .and_then(|row| row.get(field))
        .map(|s| s.to_string())
        .with_context(|| format!("No {} found in osquery output", field))
}
