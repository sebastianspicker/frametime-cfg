#![forbid(unsafe_code)]

use clap::{Parser, Subcommand};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

#[derive(Debug, Parser)]
#[command(name = "xtask", about = "Northclock repository checks")]
struct Arguments {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Reject private, generated, binary, and legacy product material.
    Hygiene {
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
    /// Check relative Markdown links in the public documentation.
    Docs {
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
}

fn main() {
    let arguments = Arguments::parse();
    let result = match arguments.command {
        Command::Hygiene { root } => hygiene(&root),
        Command::Docs { root } => docs(&root),
    };
    if let Err(errors) = result {
        for error in errors {
            eprintln!("{error}");
        }
        std::process::exit(1);
    }
}

fn hygiene(root: &Path) -> Result<(), Vec<String>> {
    let mut failures = Vec::new();
    visit(root, root, &mut |path, file_type| {
        let relative = path.strip_prefix(root).unwrap_or(path);
        let name = path.file_name().and_then(OsStr::to_str).unwrap_or("");
        let lower_name = name.to_ascii_lowercase();
        if file_type.is_symlink() {
            failures.push(format!(
                "public-tree symlink is not allowed: {}",
                relative.display()
            ));
            return false;
        }
        if file_type.is_dir() {
            if is_ignored_cargo_output(relative) {
                return false;
            }
            if is_forbidden_directory(&lower_name) {
                failures.push(format!(
                    "generated/private directory: {}",
                    relative.display()
                ));
                return false;
            }
            return true;
        }
        if lower_name.contains("ledger")
            || lower_name.contains("agent")
            || lower_name.contains("reverse-engineer")
            || lower_name.ends_with(".sarif")
        {
            failures.push(format!("private process artifact: {}", relative.display()));
        }
        if let Some(extension) = path.extension().and_then(OsStr::to_str) {
            let extension = extension.to_ascii_lowercase();
            if matches!(
                extension.as_str(),
                "exe"
                    | "dll"
                    | "pdb"
                    | "dmp"
                    | "dump"
                    | "pdf"
                    | "log"
                    | "ini"
                    | "cs"
                    | "csproj"
                    | "sln"
                    | "cpp"
                    | "cxx"
                    | "cc"
                    | "c"
                    | "h"
                    | "hpp"
                    | "ps1"
                    | "zip"
                    | "7z"
            ) {
                failures.push(format!(
                    "forbidden public file type: {}",
                    relative.display()
                ));
            }
            if extension == "rs" {
                match std::fs::read_to_string(path) {
                    Ok(source) if source.lines().count() > 600 => failures.push(format!(
                        "Rust source exceeds 600 lines ({}): {}",
                        source.lines().count(),
                        relative.display()
                    )),
                    Ok(_) => {}
                    Err(error) => failures.push(format!(
                        "could not inspect Rust source {}: {error}",
                        relative.display()
                    )),
                }
            }
        }
        true
    });
    if failures.is_empty() {
        println!("public-tree hygiene passed");
        Ok(())
    } else {
        Err(failures)
    }
}

fn is_ignored_cargo_output(relative: &Path) -> bool {
    relative.as_os_str() == OsStr::new("target")
}

fn is_forbidden_directory(lower_name: &str) -> bool {
    matches!(
        lower_name,
        "target" | "bin" | "obj" | "testresults" | "runtime" | ".vs" | "open-hydra" | "_archive"
    )
}

fn docs(root: &Path) -> Result<(), Vec<String>> {
    let mut failures = Vec::new();
    visit(root, root, &mut |path, file_type| {
        if file_type.is_file() && path.extension() == Some(OsStr::new("md")) {
            match std::fs::read_to_string(path) {
                Ok(source) => check_markdown_links(root, path, &source, &mut failures),
                Err(error) => failures.push(format!("could not read {}: {error}", path.display())),
            }
        }
        true
    });
    if failures.is_empty() {
        println!("documentation links passed");
        Ok(())
    } else {
        Err(failures)
    }
}

fn visit(root: &Path, path: &Path, callback: &mut impl FnMut(&Path, std::fs::FileType) -> bool) {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(_) => return,
    };
    if !callback(path, metadata.file_type()) || !metadata.is_dir() {
        return;
    }
    let mut entries = match std::fs::read_dir(path) {
        Ok(entries) => entries.filter_map(Result::ok).collect::<Vec<_>>(),
        Err(_) => return,
    };
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let child = entry.path();
        if child != root {
            visit(root, &child, callback);
        }
    }
}

fn check_markdown_links(root: &Path, source_path: &Path, source: &str, failures: &mut Vec<String>) {
    let mut rest = source;
    while let Some(start) = rest.find("](") {
        rest = &rest[start + 2..];
        let Some(end) = rest.find(')') else {
            break;
        };
        let target = &rest[..end];
        rest = &rest[end + 1..];
        if target.is_empty()
            || target.starts_with('#')
            || target.starts_with("https://")
            || target.starts_with("http://")
            || target.starts_with("mailto:")
        {
            continue;
        }
        let target = target.split('#').next().unwrap_or(target);
        let parent = source_path.parent().unwrap_or(root);
        if !parent.join(target).exists() {
            failures.push(format!(
                "broken relative link in {}: {target}",
                source_path.display()
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forbidden_extensions_are_explicit() {
        let path = Path::new("runtime/helper.exe");
        assert_eq!(path.extension(), Some(OsStr::new("exe")));
    }

    #[test]
    fn ignores_only_the_workspace_cargo_output_root() {
        assert!(is_ignored_cargo_output(Path::new("target")));
    }

    #[test]
    fn rejects_nested_and_noncanonical_target_directories() {
        for path in [
            Path::new("crates").join("target"),
            Path::new("nested").join("target"),
            Path::new("nested").join("..").join("target"),
            Path::new(".").join("target"),
        ] {
            assert!(
                !is_ignored_cargo_output(&path),
                "{path:?} must not be skipped"
            );
            assert!(is_forbidden_directory("target"));
        }
    }
}
