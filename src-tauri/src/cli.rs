//! Command-line arguments (startup step 4). Hand-parsed: the surface is tiny and a parser crate
//! would be a dependency for one flag.

/// Parsed command line.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CliArgs {
    /// Start without showing the Studio window (autostart, later phases).
    pub hidden: bool,
    /// Start the built-in demo track at startup (manual checks, screenshots).
    pub demo: bool,
    /// Start system-audio capture at startup (manual checks).
    pub system: bool,
    /// Arguments the app did not recognise; logged once, never fatal.
    pub unknown: Vec<String>,
}

impl CliArgs {
    pub fn parse<I: IntoIterator<Item = String>>(args: I) -> Self {
        let mut out = Self::default();
        for arg in args.into_iter().skip(1) {
            match arg.as_str() {
                "--hidden" => out.hidden = true,
                "--demo" => out.demo = true,
                "--system" => out.system = true,
                _ => out.unknown.push(arg),
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> CliArgs {
        CliArgs::parse(args.iter().map(|s| s.to_string()))
    }

    #[test]
    fn skips_the_program_name() {
        assert_eq!(parse(&["av-app"]), CliArgs::default());
    }

    #[test]
    fn reads_hidden_and_keeps_unknown_args() {
        let a = parse(&["av-app", "--hidden", "--demo", "--what"]);
        assert!(a.hidden && a.demo);
        assert_eq!(a.unknown, vec!["--what".to_string()]);
    }
}
