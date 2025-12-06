use anyhow::{Context, Result};
use clap::Parser;
use std::{collections::HashMap, path::PathBuf};
use tokio::fs;
use tokio::process::Command;

mod osquery;

use osquery::{get_host_identifier, HostIdentifier, OsqueryProvisioner};

const ENROLL_SECRET_ENV: &str = "OSQUERY_ENROLL_SECRET";

/// Hyprwatch Shadow Agent
///
/// Enrolls with a Hyprwatch server and runs osqueryd to collect system data.
/// Automatically downloads osquery if not present.
#[derive(Parser, Debug)]
#[command(name = "shadow", version, about, long_about = None)]
struct Args {
    /// Organization token for enrollment (required)
    #[arg(
        short = 't',
        long,
        env = "SHADOW_ORG_TOKEN",
        required = true
    )]
    org_token: String,

    /// Server hostname
    #[arg(
        short = 's',
        long,
        env = "SHADOW_SERVER_HOST",
        default_value = "hyprwatch.cloud"
    )]
    server: String,

    #[arg(long, env = "SHADOW_CA_CERT")]
    ca_cert: Option<PathBuf>,

    /// Data directory for osquery database and logs
    #[arg(short = 'd', long, env = "SHADOW_DATA_DIR")]
    data_dir: Option<PathBuf>,

    /// Path to osqueryd binary (skips auto-download if provided)
    #[arg(short = 'o', long, env = "OSQUERYD_PATH")]
    osqueryd_path: Option<PathBuf>,

    /// Enable verbose logging
    #[arg(short = 'v', long, env = "SHADOW_VERBOSE")]
    verbose: bool,

    /// Distributed query polling interval in seconds
    #[arg(long, default_value = "10")]
    distributed_interval: u32,

    /// Skip checksum verification when downloading osquery (development only)
    #[arg(long, hide = true)]
    skip_verify: bool,

    /// Host identifier mode: 'uuid' uses hardware UUID, 'instance' uses osquery's
    /// random instance ID (recommended for containers/VMs with duplicate hardware UUIDs)
    #[arg(long, env = "SHADOW_HOST_IDENTIFIER", default_value = "uuid")]
    host_identifier: HostIdentifier,
}

#[derive(serde::Deserialize, Debug)]
struct EnrollResponse {
    enroll_secret: String,
}

/// Get the platform-specific CA certificates path
fn get_ca_certs_path() -> &'static str {
    if cfg!(target_os = "macos") {
        "/etc/ssl/cert.pem"
    } else if cfg!(target_os = "linux") {
        "/etc/ssl/certs/ca-certificates.crt"
    } else {
        ""
    }
}

/// Get the default data directory for the platform
fn get_default_data_dir() -> PathBuf {
    if cfg!(target_os = "macos") {
        // Use user-local directory to avoid permission issues
        dirs::data_local_dir()
            .map(|d| d.join("shadow"))
            .unwrap_or_else(|| PathBuf::from("/var/lib/shadow"))
    } else if cfg!(target_os = "linux") {
        // Try user directory first, fall back to system
        dirs::data_local_dir()
            .map(|d| d.join("shadow"))
            .unwrap_or_else(|| PathBuf::from("/var/lib/shadow"))
    } else if cfg!(target_os = "windows") {
        dirs::data_local_dir()
            .map(|d| d.join("shadow"))
            .unwrap_or_else(|| PathBuf::from("C:\\ProgramData\\shadow"))
    } else {
        PathBuf::from("/var/lib/shadow")
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Resolve data directory
    let data_dir = args.data_dir.unwrap_or_else(get_default_data_dir);

    // Ensure data directory exists
    fs::create_dir_all(&data_dir)
        .await
        .context("Failed to create data directory")?;

    println!("Shadow Agent v{}", env!("CARGO_PKG_VERSION"));
    println!("─────────────────────────────────────");
    println!("  Server:    {}", args.server);
    println!("  Data dir:  {}", data_dir.display());

    // Get osqueryd path - either user-provided or auto-provisioned
    let osqueryd_path = match args.osqueryd_path {
        Some(path) => {
            // User provided a path - verify it exists
            if !path.exists() {
                anyhow::bail!("osqueryd not found at {:?}", path);
            }
            println!("  osquery:   {} (user-provided)", path.display());
            path
        }
        None => {
            // Auto-provision osquery
            let provisioner =
                OsqueryProvisioner::new(data_dir.clone()).skip_verification(args.skip_verify);
            provisioner.ensure_provisioned().await?
        }
    };

    // Create log directory
    let log_path = data_dir.join("osquery_logs");
    fs::create_dir_all(&log_path)
        .await
        .context("Failed to create log directory")?;

    // Get host identifier from osquery
    print!("  Host ID:   ");
    let host_id = get_host_identifier(&osqueryd_path, &args.host_identifier, &data_dir).await?;
    println!("{} ({})", host_id, args.host_identifier);
    println!();

    // Enroll with the server
    println!("Enrolling with server...");

    let enroll_url = format!("https://{}/api/shadow/enroll", args.server);
    let mut map = HashMap::new();
    map.insert("host_id", host_id.as_str());
    map.insert("org_token", args.org_token.as_str());

    let client = if let Some(ca_path) = &args.ca_cert {
        let cert_pem = fs::read(&ca_path).await?;
        let cert = reqwest::Certificate::from_pem(&cert_pem)?;
        reqwest::Client::builder()
            .add_root_certificate(cert)
            .build()?
    } else {
        reqwest::Client::new()
    };
    let response = client
        .post(&enroll_url)
        .json(&map)
        .send()
        .await
        .context("Failed to connect to server")?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("Enrollment failed ({}): {}", status, body);
    }

    let res: EnrollResponse = response
        .json()
        .await
        .context("Failed to parse enrollment response")?;

    println!("Enrolled successfully!");
    println!();

    // Build osqueryd command
    let mut cmd = Command::new(&osqueryd_path);

    // TLS configuration
    cmd.arg("--config_plugin").arg("tls");
    cmd.arg("--tls_hostname").arg(&args.server);

    if let Some(ca_path) = &args.ca_cert {
        cmd.arg("--tls_server_certs").arg(ca_path);
    } else {
        let ca_certs = get_ca_certs_path();
        if !ca_certs.is_empty() && std::path::Path::new(ca_certs).exists() {
            cmd.arg("--tls_server_certs").arg(ca_certs);
        }
    }

    // Enrollment
    cmd.arg("--enroll_tls_endpoint").arg("/api/osquery/enroll");
    cmd.arg("--config_tls_endpoint").arg("/api/osquery/config");
    cmd.arg("--enroll_secret_env").arg(ENROLL_SECRET_ENV);
    cmd.env(ENROLL_SECRET_ENV, res.enroll_secret);

    // Logging
    cmd.arg("--logger_plugin").arg("tls");
    cmd.arg("--logger_tls_endpoint").arg("/api/osquery/log");

    // Distributed queries
    cmd.arg("--disable_distributed").arg("false");
    cmd.arg("--distributed_plugin").arg("tls");
    cmd.arg("--distributed_interval")
        .arg(args.distributed_interval.to_string());
    cmd.arg("--distributed_tls_max_attempts").arg("10");
    cmd.arg("--distributed_tls_read_endpoint")
        .arg("/api/osquery/distributed/read");
    cmd.arg("--distributed_tls_write_endpoint")
        .arg("/api/osquery/distributed/write");

    // Paths
    cmd.arg("--pidfile").arg(data_dir.join("osquery.pid"));
    cmd.arg("--logger_path").arg(&log_path);
    cmd.arg("--database_path").arg(data_dir.join("osquery.db"));

    // Host identification - must match what we enrolled with
    cmd.arg("--host_identifier").arg(args.host_identifier.as_osquery_arg());

    // Verbose logging
    if args.verbose {
        cmd.arg("--verbose").arg("true");
        cmd.arg("--logger_stderr").arg("true");
    }

    println!("Starting osqueryd...");
    if args.verbose {
        println!("(verbose mode enabled)");
    }

    cmd.spawn()
        .context("Failed to start osqueryd")?
        .wait()
        .await?;

    Ok(())
}
