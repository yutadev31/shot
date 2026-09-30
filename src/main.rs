use std::io::{self, Write};

use clap::Parser;
use crossterm::{
    cursor,
    event::{self, Event, KeyCode},
    execute,
    terminal::{self, ClearType},
};
use shot::Capturer;

#[derive(Debug, Parser)]
#[command(name = "shot", about = "Capture a screenshot from a monitor")]
struct Cli {
    /// Monitor index to capture (zero-based); choose interactively when omitted
    #[arg(short, long, value_name = "INDEX")]
    monitor: Option<usize>,

    /// Capture and stitch all monitors into one image
    #[arg(short = 'a', long)]
    all: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let mut capturer = if cli.all {
        Capturer::new_for_all_monitors()?
    } else if let Some(monitor) = cli.monitor {
        Capturer::new_for_monitor(monitor)?
    } else {
        let mut capturer = Capturer::new_for_monitor_selection()?;
        if capturer.monitor_count() > 1 {
            let Some(monitor) = select_monitor(capturer.monitor_count())? else {
                println!();
                return Ok(());
            };
            capturer.select_monitor(monitor)?;
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

fn select_monitor(count: usize) -> io::Result<Option<usize>> {
    let mut stdout = io::stdout();
    println!("Select a monitor (↑/↓ or h/j/k/l, Enter to capture, Esc to cancel):");
    terminal::enable_raw_mode()?;
    let result = (|| {
        execute!(stdout, cursor::Hide)?;
        execute!(stdout, cursor::SavePosition)?;
        let mut selected = 0;

        loop {
            execute!(
                stdout,
                cursor::RestorePosition,
                terminal::Clear(ClearType::FromCursorDown)
            )?;
            for monitor in 0..count {
                execute!(stdout, cursor::MoveToColumn(0))?;
                if monitor == selected {
                    writeln!(stdout, "> Monitor {}", monitor + 1)?;
                } else {
                    writeln!(stdout, "  Monitor {}", monitor + 1)?;
                }
            }
            stdout.flush()?;

            if let Event::Key(key) = event::read()?
                && key.kind.is_press()
            {
                match key.code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        selected = selected.checked_sub(1).unwrap_or(count - 1);
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        selected = (selected + 1) % count;
                    }
                    KeyCode::Left | KeyCode::Char('h') => {
                        selected = selected.checked_sub(1).unwrap_or(count - 1);
                    }
                    KeyCode::Right | KeyCode::Char('l') => {
                        selected = (selected + 1) % count;
                    }
                    KeyCode::Enter => break Ok(Some(selected)),
                    KeyCode::Esc | KeyCode::Char('q') => break Ok(None),
                    _ => {}
                }
            }
        }
    })();
    let cleanup = terminal::disable_raw_mode().and(execute!(stdout, cursor::Show));
    match cleanup {
        Ok(()) => result,
        Err(error) => Err(error),
    }
}
