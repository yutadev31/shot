use std::io::{self, Write};

use crossterm::{
    cursor,
    event::{self, Event, KeyCode},
    execute,
    terminal::{self, ClearType},
};

pub(crate) fn select(names: &[String]) -> io::Result<Option<usize>> {
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
            for (monitor, name) in names.iter().enumerate() {
                execute!(stdout, cursor::MoveToColumn(0))?;
                if monitor == selected {
                    writeln!(stdout, "> {name}")?;
                } else {
                    writeln!(stdout, "  {name}")?;
                }
            }
            stdout.flush()?;

            if let Event::Key(key) = event::read()?
                && key.kind.is_press()
            {
                match key.code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        selected = selected.checked_sub(1).unwrap_or(names.len() - 1);
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        selected = (selected + 1) % names.len();
                    }
                    KeyCode::Left | KeyCode::Char('h') => {
                        selected = selected.checked_sub(1).unwrap_or(names.len() - 1);
                    }
                    KeyCode::Right | KeyCode::Char('l') => {
                        selected = (selected + 1) % names.len();
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
