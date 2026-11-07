use clap::Parser;
use shot::capturer::Capturer;

#[derive(Parser)]
#[command(name = "shot", version, about = "Capture a screenshot from a monitor")]
struct Cli {
    /// Monitor ID to capture (for example HDMI-A-1); choose interactively when omitted
    #[arg(short = 'm', long, value_name = "ID")]
    monitor: Option<String>,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("Error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    if let Some(monitor) = &cli.monitor {
        if let Ok(index) = monitor.parse::<usize>() {
            Capturer::capture_output(index)?;
        } else {
            Capturer::capture_output_by_name(monitor)?;
        }
    } else {
        Capturer::capture_all_outputs()?;
    }

    Ok(())
}
