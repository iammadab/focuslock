use clap::Parser;
use url::Url;

#[derive(Parser, Debug)]
#[command(name = "focuslock")]
#[command(about = "Fullscreen focus window with a timer overlay", long_about = None)]
struct Args {
    #[arg(long)]
    url: String,

    #[arg(long, default_value_t = 0)]
    minutes: u64,

    #[arg(long, default_value_t = 0)]
    seconds: u64,

    #[arg(long)]
    escape_key: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub url: Url,
    pub total_seconds: u64,
    pub escape_key: Option<String>,
}

impl Config {
    pub fn from_args() -> Result<Self, String> {
        let args = Args::parse();
        let escape_key = args
            .escape_key
            .or_else(|| std::env::var("FOCUSLOCK_ESCAPE_KEY").ok());
        let total_seconds = args.minutes.saturating_mul(60).saturating_add(args.seconds);
        if total_seconds == 0 {
            return Err(
                "total duration must be greater than 0 (use --minutes and/or --seconds)"
                    .to_string(),
            );
        }

        let url = parse_url(&args.url)?;
        Ok(Self {
            url,
            total_seconds,
            escape_key,
        })
    }
}

fn parse_url(raw: &str) -> Result<Url, String> {
    let url = Url::parse(raw).map_err(|err| format!("Invalid URL: {err}"))?;
    match url.scheme() {
        "http" | "https" => Ok(url),
        _ => Err("URL must start with http:// or https://".to_string()),
    }
}
