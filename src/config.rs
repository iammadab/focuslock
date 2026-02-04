use clap::Parser;
#[derive(Parser, Debug)]
#[command(name = "focuslock")]
#[command(about = "Fullscreen focus window with a timer overlay", long_about = None)]
struct Args {
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
}

#[derive(Debug, Clone)]
pub struct Config {
    pub target: RunTarget,
    pub total_seconds: u64,
    pub escape_key: Option<String>,
}

#[derive(Debug, Clone)]
pub enum RunTarget {
    App {
        app_cmd: String,
        app_timeout_ms: u64,
    },
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

        let target = match args.app_cmd {
            Some(app_cmd) => RunTarget::App {
                app_cmd,
                app_timeout_ms: args.app_timeout_ms,
            },
            None => {
                return Err("must provide --app-cmd".to_string());
            }
        };

        Ok(Self {
            target,
            total_seconds,
            escape_key,
        })
    }
}
