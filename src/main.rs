use clap::Parser;
use shot::capturer::Capturer;

#[derive(Parser)]
#[command(name = "shot", version, about = "Capture a screenshot from a monitor")]
struct Cli {
    /// Monitor ID to capture (for example HDMI-A-1); choose interactively when omitted
    #[arg(short = 'm', long, value_name = "ID")]
    monitor: Option<String>,

    /// Desktop region in slurp format: X,Y WIDTHxHEIGHT
    #[arg(short = 'r', long, num_args = 1..=2, allow_hyphen_values = true,
        value_name = "REGION", conflicts_with = "monitor")]
    region: Option<Vec<String>>,
}

/// Reports capture failures and exits with a nonzero status.
fn main() {
    if let Err(error) = run() {
        eprintln!("Error: {error}");
        std::process::exit(1);
    }
}

/// Dispatches the requested monitor or desktop-region capture.
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    if let Some(region) = &cli.region {
        Capturer::capture_desktop_region(parse_region(&region.join(" "))?)?;
    } else if let Some(monitor) = &cli.monitor {
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

/// Parses slurp's logical rectangle and rejects malformed or empty dimensions.
fn parse_region(value: &str) -> Result<(i32, i32, u32, u32), &'static str> {
    let invalid = "invalid region: expected X,Y WIDTHxHEIGHT with positive dimensions";
    let mut parts = value.split_whitespace();
    let (x, y) = parts
        .next()
        .and_then(|part| part.split_once(','))
        .ok_or(invalid)?;
    let (width, height) = parts
        .next()
        .and_then(|part| part.split_once('x'))
        .ok_or(invalid)?;

    if parts.next().is_some() {
        return Err(invalid);
    }

    let region = (
        x.parse().map_err(|_| invalid)?,
        y.parse().map_err(|_| invalid)?,
        width.parse().map_err(|_| invalid)?,
        height.parse().map_err(|_| invalid)?,
    );

    if region.2 == 0 || region.3 == 0 {
        return Err(invalid);
    }

    Ok(region)
}

#[cfg(test)]
mod tests {
    use super::{Cli, parse_region};
    use clap::Parser;

    #[test]
    /// Checks both shell argument forms of a slurp selection.
    fn accepts_slurp_output_with_or_without_quotes() {
        for args in [
            vec!["shot", "-r", "-100,20", "300x200"],
            vec!["shot", "--region", "-100,20 300x200"],
        ] {
            let cli = Cli::try_parse_from(args).unwrap();
            assert_eq!(
                parse_region(&cli.region.unwrap().join(" ")),
                Ok((-100, 20, 300, 200))
            );
        }
    }

    #[test]
    /// Rejects invalid dimensions, overflow, extra fields, and monitor conflicts.
    fn rejects_invalid_regions() {
        for value in [
            "",
            "1,2",
            "1,2 0x10",
            "1,2 10x0",
            "1,2 -1x10",
            "1,2 10x20 extra",
            "2147483648,0 1x1",
            "0,0 4294967296x1",
        ] {
            assert!(parse_region(value).is_err(), "{value}");
        }

        assert!(Cli::try_parse_from(["shot", "-m", "0", "-r", "0,0 10x10"]).is_err());
    }
}
