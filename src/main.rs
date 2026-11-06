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

    let mut captureer = if let Some(monitor) = &cli.monitor {
        if let Ok(index) = monitor.parse::<usize>() {
            Capturer::new_for_monitor(index)?
        } else {
            Capturer::new_for_monitor_name(monitor)?
        }
    } else {
        Capturer::new_for_all_monitors()?
    };

    if cli.monitor.is_some() {
        captureer.capture_output()?;
    } else {
        captureer.capture_all_outputs()?;
    }

    Ok(())
}
