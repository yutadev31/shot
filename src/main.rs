use clap::Parser;
use shot::Capturer;

#[derive(Debug, Parser)]
#[command(name = "shot", about = "Capture a screenshot from a monitor")]
struct Cli {
    /// Monitor index to capture (zero-based)
    #[arg(short, long, default_value_t = 0, value_name = "INDEX")]
    monitor: usize,

    /// Capture and stitch all monitors into one image
    #[arg(short = 'a', long)]
    all: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let mut capturer = if cli.all {
        Capturer::new_for_all_monitors()?
    } else {
        Capturer::new_for_monitor(cli.monitor)?
    };
    if cli.all {
        capturer.capture_all_outputs()?;
    } else {
        capturer.capture_output()?;
    }
    Ok(())
}
