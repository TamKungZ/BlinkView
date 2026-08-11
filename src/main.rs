#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]

mod cache;
mod config;
mod ipc;
mod media;
mod render;
mod startup;
mod thumbnail;
mod video;
mod viewer;

use config::{Config, ParseResult};
use ipc::InstanceRole;
use std::path::PathBuf;
use std::sync::mpsc;
use viewer::ViewerOutcome;

fn main() {
    if let Err(err) = real_main() {
        eprintln!("BlinkView: {err}");
        std::process::exit(1);
    }
}

fn real_main() -> Result<(), String> {
    let cfg = match Config::parse()? {
        ParseResult::Help => {
            print!("{}", config::help_text());
            return Ok(());
        }
        ParseResult::Thumbnail { input, output, size } => {
            thumbnail::create(&input, &output, size)?;
            return Ok(());
        }
        ParseResult::Startup(enabled) => {
            startup::set_enabled(enabled)?;
            println!(
                "BlinkView background startup {}.",
                if enabled { "enabled" } else { "disabled" }
            );
            return Ok(());
        }
        ParseResult::Run(cfg) => cfg,
    };

    if cfg.initial_path.is_none() && !cfg.background {
        print!("{}", config::help_text());
        return Ok(());
    }

    let (ipc_tx, ipc_rx) = mpsc::channel::<PathBuf>();
    match ipc::become_primary_or_forward(cfg.port, cfg.initial_path.as_deref(), ipc_tx)
        .map_err(|e| format!("single-instance IPC failed: {e}"))?
    {
        InstanceRole::Forwarded => return Ok(()),
        InstanceRole::Primary => {}
    }

    let mut pending = cfg.initial_path.clone();
    loop {
        let path = match pending.take() {
            Some(path) => path,
            None => match ipc_rx.recv() {
                Ok(path) => path,
                Err(_) => break,
            },
        };

        match viewer::run(path, &cfg, &ipc_rx) {
            Ok(ViewerOutcome::Quit) => break,
            Ok(ViewerOutcome::Hidden) => {
                if !cfg.background {
                    break;
                }
            }
            Err(err) => {
                eprintln!("BlinkView viewer error: {err}");
                if !cfg.background {
                    return Err(err);
                }
            }
        }
    }

    Ok(())
}
