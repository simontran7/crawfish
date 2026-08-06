use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};

pub enum Command {
    Build { path: PathBuf, emit: Vec<EmitKind> },
    Run { path: PathBuf, emit: Vec<EmitKind> },
    Check { path: PathBuf, emit: Vec<EmitKind> },
    Help,
    Version,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmitKind {
    Ast,
    Hir,
    Mir,
    LlvmIr,
    Dot,
}

#[derive(Debug)]
pub enum CLIError {
    InvalidFilePath(String),
    InvalidFileExtension,
    InvalidCommand(String),
    MissingArgument,
    InvalidFlag(String),
    InvalidEmitKind(String),
}

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
