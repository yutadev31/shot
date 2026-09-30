use std::{
    io::{self, Write},
    process::{Command, Stdio},
};

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

    /// Select a monitor using rofi
    #[arg(short = 'r', long, conflicts_with_all = ["monitor", "all"])]
    rofi: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let mut capturer = if cli.all {
        Capturer::new_for_all_monitors()?
    } else if let Some(monitor) = cli.monitor {
        Capturer::new_for_monitor(monitor)?
    } else {
        let mut capturer = Capturer::new_for_monitor_selection()?;
        let monitor = if cli.rofi {
            select_monitor_with_rofi(capturer.monitor_count())?
        } else if capturer.monitor_count() > 1 {
            select_monitor(capturer.monitor_count())?
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

fn select_monitor_with_rofi(count: usize) -> io::Result<Option<usize>> {
    let choices = (0..count)
        .map(|monitor| format!("Monitor {}", monitor + 1))
        .collect::<Vec<_>>()
        .join("\n");

    let mut child = Command::new("rofi")
        .args(["-dmenu", "-i", "-p", "Monitor"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;
    let Some(mut stdin) = child.stdin.take() else {
        return Err(io::Error::other("failed to open rofi stdin"));
    };
    stdin.write_all(choices.as_bytes())?;

    let output = child.wait_with_output()?;
    if !output.status.success() {
        return Ok(None);
    }

    let selected = String::from_utf8(output.stdout)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let selected = selected.trim();
    let Some(index) = selected.strip_prefix("Monitor ") else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("rofi returned an invalid monitor: {selected:?}"),
        ));
    };
    let index = index.parse::<usize>().map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("rofi returned an invalid monitor: {error}"),
        )
    })?;

    index
        .checked_sub(1)
        .filter(|&index| index < count)
        .map(Some)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("rofi returned a monitor outside the available range: {index}"),
            )
        })
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
