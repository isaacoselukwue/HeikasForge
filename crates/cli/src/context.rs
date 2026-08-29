use std::sync::Arc;

use heikas_application::error::ApplicationResult;
use heikas_application::usecases::ApplicationService;
use heikas_infrastructure::{build_runtime, Runtime, StoreLayout};
use serde::Serialize;

use crate::presentation::Palette;

pub struct CommandContext {
    pub runtime: Runtime,
    pub palette: Palette,
    pub json: bool,
    pub quiet: bool,
}

impl CommandContext {
    pub fn build(
        home: Option<std::path::PathBuf>,
        json: bool,
        quiet: bool,
        plain: bool,
    ) -> ApplicationResult<Self> {
        let layout = match home {
            Some(path) => StoreLayout::new(path),
            None => StoreLayout::discover()?,
        };
        let runtime = build_runtime(layout)?;
        Ok(Self {
            runtime,
            palette: if json || plain {
                Palette::plain()
            } else {
                Palette::detect(plain)
            },
            json,
            quiet,
        })
    }

    pub fn service(&self) -> Arc<ApplicationService> {
        Arc::clone(&self.runtime.service)
    }

    pub fn emit<T: Serialize>(&self, value: &T, human: impl FnOnce(&Palette) -> String) {
        if self.json {
            match serde_json::to_string_pretty(value) {
                Ok(text) => println!("{text}"),
                Err(error) => eprintln!("the response could not be encoded: {error}"),
            }
        } else if !self.quiet {
            print!("{}", human(&self.palette));
        }
    }

    pub fn note(&self, message: &str) {
        if !self.json && !self.quiet {
            println!("{}", self.palette.muted(message));
        }
    }
}
