use std::path::PathBuf;

use clap::Parser;
use shot::{Capturer, serve_clipboard};

mod selector;

#[derive(Debug, Parser)]
#[command(name = "shot", about = "Capture a screenshot from a monitor")]
struct Cli {
    /// Monitor ID to capture (for example HDMI-A-1); choose interactively when omitted
    #[arg(short = 'm', long, value_name = "ID")]
    monitor: Option<String>,

    /// Capture and stitch all monitors into one image
    #[arg(short = 'a', long)]
    all: bool,

    /// Select a monitor using rofi
    #[arg(short = 'r', long, conflicts_with_all = ["monitor", "all"])]
    rofi: bool,

    #[arg(long, hide = true, value_name = "PATH")]
    clipboard_daemon: Option<PathBuf>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    if let Some(path) = cli.clipboard_daemon {
        serve_clipboard(&path)?;
        return Ok(());
    }

    let mut capturer = if cli.all {
        Capturer::new_for_all_monitors()?
    } else if let Some(monitor) = cli.monitor {
        if let Ok(index) = monitor.parse::<usize>() {
            Capturer::new_for_monitor(index)?
        } else {
            Capturer::new_for_monitor_name(&monitor)?
        }
    } else {
        let mut capturer = Capturer::new_for_monitor_selection()?;
        let monitor_names = capturer.monitor_names();
        let monitor = if cli.rofi {
            selector::rofi::select(&monitor_names)?
        } else if capturer.monitor_count() > 1 {
            selector::cli::select(&monitor_names)?
        } else {
            Some(0)
        };
        if let Some(monitor) = monitor {
            capturer.select_monitor(monitor)?;
        } else {
            println!();
            return Ok(());
        }
        capturer
    };
    if cli.all {
        capturer.capture_all_outputs()?;
    } else {
        capturer.capture_output()?;
    }
    Ok(())
}
