use clap::Parser;
use std::io::ErrorKind;
use std::path::PathBuf;
use url::Url;
#[derive(Parser, Debug)]
#[command(name = "focuslock")]
#[command(about = "Fullscreen focus window with a timer overlay", long_about = None)]
struct Args {
    #[arg(long)]
    url: Option<String>,

    #[arg(long)]
    app_cmd: Option<String>,

    #[arg(long, default_value_t = 4000)]
    app_timeout_ms: u64,

    #[arg(long, default_value_t = 0)]
    minutes: u64,

    #[arg(long, default_value_t = 0)]
    seconds: u64,

    #[arg(long)]
    escape_key: Option<String>,

    #[arg(long)]
    allow_classes: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub target: RunTarget,
    pub total_seconds: u64,
    pub escape_key: Option<String>,
    pub allow_classes: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum RunTarget {
    Web {
        url: Url,
        app_timeout_ms: u64,
    },
    App {
        app_cmd: String,
        app_timeout_ms: u64,
    },
}

impl Config {
    pub fn from_args() -> Result<Self, String> {
        let args = Args::parse();
        let escape_key = match args.escape_key {
            Some(key) => Some(key),
            None => read_escape_key_file()?,
        };
        let total_seconds = args.minutes.saturating_mul(60).saturating_add(args.seconds);
        if total_seconds == 0 {
            return Err(
                "total duration must be greater than 0 (use --minutes and/or --seconds)"
                    .to_string(),
            );
        }
        let allow_classes = parse_allow_classes(args.allow_classes.as_deref());

        let target = match (args.url, args.app_cmd) {
            (Some(url), None) => RunTarget::Web {
                url: parse_url(&url)?,
                app_timeout_ms: args.app_timeout_ms,
            },
            (None, Some(app_cmd)) => RunTarget::App {
                app_cmd,
                app_timeout_ms: args.app_timeout_ms,
            },
            (None, None) => {
                return Err("must provide --url or --app-cmd".to_string());
            }
            (Some(_), Some(_)) => {
                return Err("use either --url or --app-cmd, not both".to_string());
            }
        };

        Ok(Self {
            target,
            total_seconds,
            escape_key,
            allow_classes,
        })
    }
}

fn parse_allow_classes(raw: Option<&str>) -> Vec<String> {
    let Some(raw) = raw else {
        return Vec::new();
    };
    raw.split(',')
        .map(|entry| entry.trim())
        .filter(|entry| !entry.is_empty())
        .map(|entry| entry.to_lowercase())
        .collect()
}

fn read_escape_key_file() -> Result<Option<String>, String> {
    let path = escape_key_path()?;
    match std::fs::read_to_string(&path) {
        Ok(contents) => {
            let trimmed = contents.trim();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                Ok(Some(trimmed.to_string()))
            }
        }
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(None),
        Err(err) => Err(format!(
            "failed to read escape key file {}: {err}",
            path.display()
        )),
    }
}

fn escape_key_path() -> Result<PathBuf, String> {
    let config_home = std::env::var("XDG_CONFIG_HOME")
        .ok()
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|home| PathBuf::from(home).join(".config"))
        });
    match config_home {
        Some(config_home) => Ok(config_home.join("focuslock").join("focuslock.key")),
        None => Err("XDG_CONFIG_HOME and HOME are not set".to_string()),
    }
}

fn parse_url(raw: &str) -> Result<Url, String> {
    let url = Url::parse(raw).map_err(|err| format!("Invalid URL: {err}"))?;
    match url.scheme() {
        "http" | "https" => Ok(url),
        _ => Err("URL must start with http:// or https://".to_string()),
    }
}
