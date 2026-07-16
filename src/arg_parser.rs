use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};

/// A parsed CLI invocation, returned by [`parse_args`].
pub enum Command {
    /// Compile the source file at this path.
    Compile(PathBuf),
    /// Print usage information (`-h`/`--help`).
    Help,
    /// Print the compiler's version (`-v`/`--version`).
    Version,
}

/// An error encountered while parsing command-line arguments.
#[derive(Debug)]
pub enum CLIError {
    /// The given path does not point to an existing file.
    InvalidFilePath(String),
    /// The given file does not have a `.crw` extension.
    InvalidFileExtension,
    /// The first argument isn't a recognized command or flag.
    InvalidCommand(String),
    /// A required argument (e.g. the source path for `compile`) was not given.
    MissingArgument,
}

/// Parses `args` (as received from [`std::env::args`], including the
/// program name at index 0) into a [`Command`].
///
/// # Examples
///
/// ```rust,ignore
/// let args: Vec<String> = vec!["crawfish".into(), "compile".into(), "main.crw".into()];
/// let command = parse_args(&args).unwrap();
/// ```
pub fn parse_args(args: &[String]) -> Result<Command, CLIError> {
    let mut args_iter = args.iter().skip(1);
    let command = args_iter.next().ok_or(CLIError::MissingArgument)?;
    match command.as_str() {
        "compile" => {
            let path_str = args_iter.next().ok_or(CLIError::MissingArgument)?;
            let source_path = PathBuf::from(path_str);
            if !source_path.is_file() {
                return Err(CLIError::InvalidFilePath(path_str.to_owned()));
            }
            if !has_crw_extension(&source_path) {
                return Err(CLIError::InvalidFileExtension);
            }
            Ok(Command::Compile(source_path))
        }
        "-h" | "--help" => Ok(Command::Help),
        "-v" | "--version" => Ok(Command::Version),
        other => Err(CLIError::InvalidCommand(other.to_owned())),
    }
}

fn has_crw_extension(p: &Path) -> bool {
    p.extension().and_then(|s| s.to_str()) == Some("crw")
}

impl fmt::Display for CLIError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFilePath(path) => write!(f, "Invalid file path: {path}"),
            Self::InvalidFileExtension => write!(f, "Invalid file extension (expected .crw)"),
            Self::InvalidCommand(cmd) => write!(f, "Invalid command: {cmd}"),
            Self::MissingArgument => write!(f, "Missing arguments. Use --help for more info."),
        }
    }
}

impl Error for CLIError {}
