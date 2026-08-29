use heikas_api::{start, ServerOptions};
use heikas_application::error::ApplicationResult;
use serde::Serialize;

use crate::context::CommandContext;
use crate::exit::ExitCode;

#[derive(Debug, Serialize)]
pub struct InterfaceOutcome {
    pub address: String,
    pub bootstrap_url: String,
    pub demonstration_mode: bool,
    pub interface_embedded: bool,
}

pub struct InterfaceOptions {
    pub run: Option<String>,
    pub port: u16,
    pub open_browser: bool,
    pub demonstration: bool,
    pub bind_all_interfaces: bool,
}

pub async fn serve(
    context: &CommandContext,
    options: InterfaceOptions,
) -> ApplicationResult<ExitCode> {
    let server = start(
        context.runtime.clone(),
        ServerOptions {
            port: options.port,
            bind_all_interfaces: options.bind_all_interfaces,
            demonstration_mode: options.demonstration,
        },
    )
    .await?;

    let mut bootstrap_url = server.bootstrap_url.clone();
    if let Some(reference) = &options.run {
        let run_id = context.service().resolve_run_reference(reference).await?;
        bootstrap_url = format!(
            "{}&run={run_id}",
            server.bootstrap_url
        );
    }

    let outcome = InterfaceOutcome {
        address: server.address.to_string(),
        bootstrap_url: bootstrap_url.clone(),
        demonstration_mode: options.demonstration,
        interface_embedded: heikas_api::assets::interface_is_embedded(),
    };
    context.emit(&outcome, |palette| {
        let mut text = String::new();
        text.push_str(&palette.heading("Local interface started\n"));
        text.push_str(&format!("Address: http://{}\n", outcome.address));
        text.push_str(&format!("Open: {bootstrap_url}\n"));
        if outcome.demonstration_mode {
            text.push_str(&palette.warning("Demonstration mode is active.\n"));
        }
        if !outcome.interface_embedded {
            text.push_str(&palette.warning(
                "The interface bundle is not embedded in this build. Build the web application first.\n",
            ));
        }
        text.push_str(&palette.muted("Press Ctrl+C to stop.\n"));
        text
    });

    if options.open_browser {
        open_in_browser(&bootstrap_url);
    }

    tokio::signal::ctrl_c().await.ok();
    context.note("Stopping the local interface.");
    server.shutdown().await;
    Ok(ExitCode::Success)
}

fn open_in_browser(url: &str) {
    let candidates: Vec<(&str, Vec<String>)> = if cfg!(target_os = "macos") {
        vec![("open", vec![url.to_string()])]
    } else if cfg!(target_os = "windows") {
        vec![(
            "cmd",
            vec!["/C".to_string(), "start".to_string(), String::new(), url.to_string()],
        )]
    } else {
        vec![
            ("xdg-open", vec![url.to_string()]),
            ("gio", vec!["open".to_string(), url.to_string()]),
        ]
    };
    for (program, arguments) in candidates {
        if std::process::Command::new(program)
            .args(&arguments)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .is_ok()
        {
            return;
        }
    }
}
