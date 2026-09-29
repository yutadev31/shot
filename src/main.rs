use clap::Parser;
use shot::Capturer;

#[derive(Debug, Parser)]
#[command(name = "shot", about = "Capture a screenshot from a monitor")]
struct Cli {
    /// Monitor index to capture (zero-based)
    #[arg(short, long, default_value_t = 0, value_name = "INDEX")]
    monitor: usize,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let mut capturer = Capturer::new_for_monitor(cli.monitor)?;
    capturer.capture_output()?;
    Ok(())
}
