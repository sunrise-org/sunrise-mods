mod builds;
mod dependency;
mod fit;
mod game_build;
mod github;
mod index;
mod index_signature;
mod jar;
mod moved;
mod registry;
mod release_payload;
mod schema;
mod signed;
mod tree;
mod watch;

use std::{
    ffi::{OsStr, OsString},
    path::Path,
    process::{Command, ExitCode},
};

use time::{OffsetDateTime, format_description::well_known::Rfc3339};

const USAGE: &str = "usage: registry-bot check <checkout>
       registry-bot index <checkout> --serial <n> --out <dir> [--generated-fit]
       registry-bot watch <checkout> <registry-owner/repo> <out>
       registry-bot collection-author <base checkout> <head checkout> <PR author login>
       registry-bot stay-listed <base checkout> <head checkout>
       registry-bot registration <base checkout> <head checkout>
       registry-bot moved <base checkout> <head checkout> <registry-owner/repo>
       registry-bot verify-release <checkout> <registry-owner/repo> <project> <release.toml> <out>
       registry-bot builds --out <file>
       registry-bot fit <checkout> [--builds <file written by builds>]
       registry-bot jars <checkout>
       registry-bot release-fit <builds.json> <release.toml> --out <file>
       registry-bot verify-index <index> <sig> --key <pub>";

fn main() -> ExitCode {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    match args.as_slice() {
        [command, checkout, repo, out] if command == "watch" => {
            return finished(with_github(repo, |gh| {
                watch::run(Path::new(checkout), Path::new(out), gh)
            }));
        }
        [command, base, head, repo] if command == "moved" => {
            return finished(with_github(repo, |gh| {
                print!("{}", moved::report(Path::new(base), Path::new(head), gh)?);
                Ok(())
            }));
        }
        [command, checkout, repo, id, row, out] if command == "verify-release" => {
            return finished(with_github(repo, |gh| {
                let id = schema::ProjectId::parse(
                    id.to_str().ok_or("project ID is not UTF-8")?.to_owned(),
                )?;
                watch::check(Path::new(checkout), &id, Path::new(row), Path::new(out), gh)
            }));
        }
        _ => (),
    }

    let result = match args.as_slice() {
        [command, checkout] if command == "check" => registry::load(Path::new(checkout))
            .map(|_| ())
            .map_err(|e| e.join("\n")),
        [command, base, head] if command == "stay-listed" => {
            registry::stay_listed(Path::new(base), Path::new(head))
        }
        [command, base, head] if command == "registration" => {
            registry::registration(Path::new(base), Path::new(head))
        }
        [command, base, head, login] if command == "collection-author" => match login.to_str() {
            Some(login) => registry::collection_author(Path::new(base), Path::new(head), login),
            None => Err("login is not UTF-8".to_owned()),
        },
        [
            command,
            checkout,
            serial_flag,
            serial,
            out_flag,
            out,
            tail @ ..,
        ] if command == "index" && serial_flag == "--serial" && out_flag == "--out" => {
            let Some(serial) = serial.to_str().and_then(|s| s.parse::<u64>().ok()) else {
                eprintln!("--serial expects a nonnegative integer");
                return ExitCode::from(2);
            };
            let generated_fit = match tail {
                [] => false,
                [flag] if flag == "--generated-fit" => true,
                _ => return usage(),
            };
            write_index(Path::new(checkout), serial, Path::new(out), generated_fit)
        }
        [command, out_flag, out] if command == "builds" && out_flag == "--out" => {
            builds::discover(Path::new(out))
        }
        [command, checkout] if command == "fit" => fit::run(Path::new(checkout), None),
        [command, checkout, builds_flag, builds]
            if command == "fit" && builds_flag == "--builds" =>
        {
            fit::run(Path::new(checkout), Some(Path::new(builds)))
        }
        [command, checkout] if command == "jars" => {
            jar::availability(Path::new(checkout), &jar::Curl)
        }
        [command, builds, row, out_flag, out]
            if command == "release-fit" && out_flag == "--out" =>
        {
            fit::release(Path::new(builds), Path::new(row), Path::new(out))
        }
        [command, index, sig, flag, pubkey] if command == "verify-index" && flag == "--key" => {
            signed::verify(Path::new(index), Path::new(sig), Path::new(pubkey)).map(|_| ())
        }
        [help] if help == OsStr::new("--help") || help == OsStr::new("-h") => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        _ => return usage(),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            if args.first().is_some_and(|command| command == "check") {
                // Layout diagnostics have already been escaped and arranged one per line.
                eprintln!("{error}");
            } else {
                eprintln!("{}", escape(&error));
            }
            failure()
        }
    }
}

/// A failed run during which GitHub was unavailable exits apart from one that only reached
/// a refusal: what it refused may be the fault of the failed lookup.
fn failure() -> ExitCode {
    if github::was_unavailable() {
        ExitCode::from(github::UNAVAILABLE_EXIT)
    } else {
        ExitCode::FAILURE
    }
}

fn usage() -> ExitCode {
    eprintln!("{USAGE}");
    ExitCode::from(2)
}

fn write_index(
    checkout: &Path,
    serial: u64,
    out: &Path,
    generated_fit: bool,
) -> Result<(), String> {
    let load = if generated_fit {
        registry::load_generated
    } else {
        registry::load
    };
    let registry = load(checkout).map_err(|e| e.join("; "))?;
    let generated = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|e| e.to_string())?;
    index::write(&registry, checkout, serial, &generated, out)
        .map_err(|e| format!("{}: {e}", out.join("index.json").display()))
}

fn escape(reason: &str) -> String {
    reason
        .escape_debug()
        .to_string()
        .replace("::", "%3A%3A")
        .replace("##[", "%23%23%5B")
}

fn command_output(cmd: &mut Command) -> Result<Vec<u8>, String> {
    let name = OsString::from(cmd.get_program());
    let output = cmd
        .output()
        .map_err(|e| format!("{}: {e}", name.to_string_lossy()))?;
    if !output.status.success() {
        return Err(format!(
            "{} failed ({}): {}",
            name.to_string_lossy(),
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(output.stdout)
}

fn with_github(
    repo: &OsStr,
    run: impl FnOnce(&github::Cli<'_>) -> Result<(), String>,
) -> Result<(), String> {
    let name = schema::RepositoryName::parse(
        repo.to_str()
            .ok_or("repository name is not UTF-8")?
            .to_owned(),
    )?;
    run(&github::Cli { registry: &name })
}

fn finished(result: Result<(), String>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{}", watch::diagnostic(&error));
            failure()
        }
    }
}
