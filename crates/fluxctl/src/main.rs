//! FluxWM IPC command-line client. Exit 2: usage; 3: connection/readiness;
//! 4: rejected request; 5: protocol failure.
use flux_ipc::{ErrorCode, Method, Outcome, Request};
use std::{path::PathBuf, process::ExitCode};
fn run() -> Result<(), (u8, String)> {
    let mut args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!(
            "fluxctl [--socket PATH] query <version|capabilities|windows.list|workspaces.list|outputs.list|config.effective|diagnostics>\nfluxctl [--socket PATH] action '<JSON flux-core action>'\nfluxctl [--socket PATH] config-check [PATH]\nfluxctl [--socket PATH] reload\nSocket: --socket or FLUXWM_SOCKET. No ambiguous auto-discovery."
        );
        return Ok(());
    }
    let socket = if args.first().is_some_and(|a| a == "--socket") {
        if args.len() < 3 {
            return Err((2, "--socket requires PATH and a command".into()));
        }
        args.remove(0);
        PathBuf::from(args.remove(0))
    } else {
        std::env::var_os("FLUXWM_SOCKET")
            .map(PathBuf::from)
            .ok_or((
                3,
                "FLUXWM_SOCKET is unset; provide --socket PATH for a ready compositor instance"
                    .into(),
            ))?
    };
    let command = args.first().map(String::as_str).unwrap_or("");
    let mut request = Request::new(format!("fluxctl-{}", std::process::id()), Method::Version);
    match command {
        "query" if args.len() == 2 => {
            request.method = match args[1].as_str() {
                "version" => Method::Version,
                "capabilities" => Method::Capabilities,
                "windows.list" => Method::Windows,
                "workspaces.list" => Method::Workspaces,
                "outputs.list" => Method::Outputs,
                "config.effective" => Method::EffectiveConfig,
                "diagnostics" => Method::Diagnostics,
                _ => return Err((2, "unknown query method; use --help".into())),
            };
        }
        "action" if args.len() == 2 => {
            request.method = Method::Action;
            request.params =
                flux_ipc::decode(args[1].as_bytes()).map_err(|e| (2, e.to_string()))?;
        }
        "config-check" if args.len() <= 2 => {
            request.method = Method::ConfigCheck;
            if let Some(path) = args.get(1) {
                request.params = serde_json::json!({"path":path});
            }
        }
        "reload" if args.len() == 1 => request.method = Method::ConfigReload,
        _ => return Err((2, "invalid arguments; use --help".into())),
    }
    request.validate().map_err(|e| (2, e.message))?;
    let response = flux_ipc::call(&socket, &request).map_err(|e| {
        let code = if e.kind() == std::io::ErrorKind::InvalidData {
            5
        } else {
            3
        };
        (code, format!("{}: {e}", socket.display()))
    })?;
    let json = serde_json::to_string_pretty(&response).map_err(|e| (5, e.to_string()))?;
    println!("{json}");
    if let Outcome::Failure { error } = response.outcome {
        return Err((
            if error.code == ErrorCode::NotReady {
                3
            } else {
                4
            },
            error.message,
        ));
    }
    Ok(())
}
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err((code, message)) => {
            eprintln!("fluxctl: {message}");
            ExitCode::from(code)
        }
    }
}
