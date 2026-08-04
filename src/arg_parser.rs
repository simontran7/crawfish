use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};

/// A parsed CLI invocation, returned by [`parse_args`].
pub enum Command {
    /// Build the source file at this path into an executable.
    Build { path: PathBuf, emit: Vec<EmitKind> },
    /// Compile and run the source file at this path.
    Run { path: PathBuf, emit: Vec<EmitKind> },
    /// Check the source file at this path without producing an executable.
    Check { path: PathBuf, emit: Vec<EmitKind> },
    /// Print usage information (`-h`/`--help`).
    Help,
    /// Print the compiler's version (`-v`/`--version`).
    Version,
}

/// An intermediate representation `--emit` can print.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmitKind {
    Ast,
    Hir,
    Mir,
    LlvmIr,
    Dot,
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
    /// A required argument (e.g. the source path for `build`) was not given.
    MissingArgument,
    /// An argument after the source path isn't a recognized flag.
    InvalidFlag(String),
    /// `--emit` was given a kind other than `ast`, `hir`, `mir`, or `llvm-ir`.
    InvalidEmitKind(String),
}

/// Parses `args` (as received from [`std::env::args`], including the
/// program name at index 0) into a [`Command`].
///
/// # Examples
///
/// ```rust,ignore
/// let args: Vec<String> = vec!["crawfish".into(), "build".into(), "main.crw".into()];
/// let command = parse_args(&args).unwrap();
/// ```
pub fn parse_args(args: &[String]) -> Result<Command, CLIError> {
    // `-h`/`--help` wins regardless of position, e.g. `crawfish run main.crw
    // --help` still shows help rather than erroring on `--help` as an
    // unrecognized flag.
    if args[1..].iter().any(|a| a == "-h" || a == "--help") {
        return Ok(Command::Help);
    }

    let mut args_iter = args.iter().skip(1);
    let command = args_iter.next().ok_or(CLIError::MissingArgument)?;
    match command.as_str() {
        "build" => {
            let (path, emit) = parse_path_and_emit(&mut args_iter)?;
            Ok(Command::Build { path, emit })
        }
        "run" => {
            let (path, emit) = parse_path_and_emit(&mut args_iter)?;
            Ok(Command::Run { path, emit })
        }
        "check" => {
            let (path, emit) = parse_path_and_emit(&mut args_iter)?;
            Ok(Command::Check { path, emit })
        }
        "-v" | "--version" => Ok(Command::Version),
        other => Err(CLIError::InvalidCommand(other.to_owned())),
    }
}

/// Parses the `<file>.crw` path and `--emit=ast,hir,mir,llvm-ir` flag shared
/// by `build`, `run`, and `check`, in either order. `emit` is empty if no
/// `--emit` was given.
fn parse_path_and_emit<'a>(
    args_iter: &mut impl Iterator<Item = &'a String>,
) -> Result<(PathBuf, Vec<EmitKind>), CLIError> {
    let mut path_str = None;
    let mut emit = Vec::new();
    for arg in args_iter {
        match arg.strip_prefix("--emit=") {
            Some(list) => {
                for kind in list.split(',') {
                    emit.push(match kind {
                        "ast" => EmitKind::Ast,
                        "hir" => EmitKind::Hir,
                        "mir" => EmitKind::Mir,
                        "llvm-ir" => EmitKind::LlvmIr,
                        "dot" => EmitKind::Dot,
                        other => return Err(CLIError::InvalidEmitKind(other.to_owned())),
                    });
                }
            }
            None if path_str.is_none() => path_str = Some(arg),
            None => return Err(CLIError::InvalidFlag(arg.to_owned())),
        }
    }

    let path_str = path_str.ok_or(CLIError::MissingArgument)?;
    let source_path = PathBuf::from(path_str);
    if !source_path.is_file() {
        return Err(CLIError::InvalidFilePath(path_str.to_owned()));
    }
    if !has_crw_extension(&source_path) {
        return Err(CLIError::InvalidFileExtension);
    }
    Ok((source_path, emit))
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
            Self::InvalidFlag(flag) => write!(f, "Invalid flag: {flag}"),
            Self::InvalidEmitKind(kind) => write!(
                f,
                "Invalid --emit kind: {kind} (expected ast, hir, mir, llvm-ir, or dot)"
            ),
        }
    }
}

impl Error for CLIError {}
