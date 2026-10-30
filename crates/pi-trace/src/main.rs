mod catalog;
mod service;
mod system_config;
mod system_routes;
use std::{error::Error, ffi::OsString, path::PathBuf};

fn source_argument(
    args: impl IntoIterator<Item = OsString>,
) -> Result<Option<PathBuf>, &'static str> {
    let args: Vec<_> = args.into_iter().collect();
    match args.as_slice() {
        [] => Ok(None),
        [command, directory]
            if (command == "serve" || command == "watch") && !directory.is_empty() =>
        {
            Ok(Some(PathBuf::from(directory)))
        }
        _ => Err("Usage: pi-trace [serve|watch <directory>]"),
    }
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let state = if args.first().is_some_and(|a| a == "system") {
        service::AppState::system(system_config::SystemConfig::resolve(
            &args[1..],
            &std::env::current_dir()?,
            &[
                "HOME",
                "CODEX_HOME",
                "PI_CODING_AGENT_DIR",
                "PI_CODING_AGENT_SESSION_DIR",
                "CLAUDE_CONFIG_DIR",
                "XDG_DATA_HOME",
                "OPENCODE_DB",
            ]
            .into_iter()
            .filter_map(|key| std::env::var(key).ok().map(|value| (key.to_owned(), value)))
            .collect(),
        )?)
    } else {
        let source = source_argument(args.clone())?;
        service::AppState::with_mode(source, args.first().is_some_and(|a| a == "watch"))
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:4317").await?;
    state.begin();
    let app = service::router(state.clone());
    println!("Pi Trace bootstrap: http://127.0.0.1:4317");
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            #[cfg(unix)]
            {
                let mut terminate =
                    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                        .expect("termination signal");
                tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = terminate.recv() => {} }
            }
            #[cfg(not(unix))]
            let _ = tokio::signal::ctrl_c().await;
            state.shutdown().await;
        })
        .await?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_arguments_are_exact_and_preserve_spaces() {
        assert_eq!(source_argument([]).unwrap(), None);
        assert_eq!(
            source_argument(["serve".into(), "recording with spaces".into()]).unwrap(),
            Some(PathBuf::from("recording with spaces"))
        );
        assert_eq!(
            source_argument(["watch".into(), "recording with spaces".into()]).unwrap(),
            Some(PathBuf::from("recording with spaces"))
        );
        for args in [
            vec!["serve"],
            vec!["bad"],
            vec!["serve", "a", "b"],
            vec!["serve", ""],
            vec!["watch"],
            vec!["watch", ""],
            vec!["watch", "a", "b"],
        ] {
            assert!(source_argument(args.into_iter().map(OsString::from)).is_err());
        }
    }
}
