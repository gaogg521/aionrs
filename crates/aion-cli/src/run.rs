use std::env;
use std::io::{self, IsTerminal};
use std::sync::Arc;

use aion_agent::engine::AgentEngine;
use aion_agent::error::AgentError;
use aion_agent::output::OutputSink;
use aion_agent::output::terminal::TerminalSink;
use aion_config::config::Config;
use aion_mcp::manager::McpManager;
use aion_tui::{TuiMetadata, TuiRuntime};

use crate::bootstrap::{build_engine, init_logging, resolve_config};
use crate::cli::Cli;
use crate::json_stream;

/// Entry point for the default (non-subcommand) invocation: validates
/// flags, resolves config/logging, then either dispatches to JSON stream
/// mode or bootstraps a terminal engine and runs a single prompt / REPL.
pub(crate) async fn run_main_flow(cli: Cli) -> anyhow::Result<()> {
    if cli.resume.is_some() && cli.session_id.is_some() {
        anyhow::bail!("Cannot use --resume and --session-id together");
    }

    let config = resolve_config(&cli)?;
    let _log_guard = init_logging(&config, cli.log_dir.as_deref(), cli.log_level.as_deref());

    let cwd = env::current_dir()?.to_string_lossy().to_string();

    // Branch to JSON stream mode
    if cli.json_stream {
        return json_stream::run(config, &cwd, cli.resume, cli.session_id, cli.fork_session).await;
    }

    let prompt = cli.prompt.join(" ");
    if prompt.is_empty() && io::stdin().is_terminal() && io::stdout().is_terminal() {
        return run_tui_flow(config, &cwd, &cli).await;
    }

    run_plain_flow(config, &cwd, &cli, &prompt).await
}

async fn run_tui_flow(config: Config, cwd: &str, cli: &Cli) -> anyhow::Result<()> {
    let provider_name = config.provider_label.clone();
    let mut tui = TuiRuntime::new(TuiMetadata {
        model: config.model.clone(),
        provider: provider_name.clone(),
        cwd: cwd.to_string(),
        no_color: cli.no_color,
    });
    let output = tui.output_sink();
    let mut resumed_messages = None;
    let fork_session = cli.fork_session;

    let result = build_engine(
        config,
        cwd,
        output.clone(),
        cli.resume.as_deref(),
        fork_session,
        |session| {
            resumed_messages = Some(session.messages.clone());
            emit_resume_banner(output.as_ref(), session, fork_session);
        },
    )
    .await?;
    let mut engine = result.engine;
    let mcp_managers = result.mcp_managers;

    if cli.resume.is_none() {
        engine.init_session(&provider_name, cwd, cli.session_id.as_deref())?;
    }

    tui.set_commands(engine.slash_commands());
    tui.set_session_id(engine.current_session_id());
    if let Some(messages) = resumed_messages {
        tui.set_history(&messages);
    }
    let run_result = tui.run(&mut engine).await;
    shutdown(&engine, &mcp_managers).await;
    run_result
}

async fn run_plain_flow(config: Config, cwd: &str, cli: &Cli, prompt: &str) -> anyhow::Result<()> {
    let terminal = Arc::new(TerminalSink::new(cli.no_color));
    let output: Arc<dyn OutputSink> = terminal.clone();

    let provider_name = config.provider_label.clone();
    let terminal_for_resume = terminal.clone();
    let fork_session = cli.fork_session;

    let result = build_engine(
        config,
        cwd,
        output.clone(),
        cli.resume.as_deref(),
        fork_session,
        |session| {
            let banner = resume_banner(session, fork_session);
            terminal_for_resume.formatter().session_info(&banner);
        },
    )
    .await?;
    let mut engine = result.engine;
    let mcp_managers = result.mcp_managers;

    if cli.resume.is_none() {
        engine.init_session(&provider_name, cwd, cli.session_id.as_deref())?;
    }

    if prompt.is_empty() {
        repl_loop(&mut engine, &terminal, &output).await?;
    } else {
        let run_result = engine.run(prompt, "").await?;
        output.emit_stream_end(
            "",
            run_result.turns,
            run_result.usage.input_tokens,
            run_result.usage.output_tokens,
            run_result.usage.cache_creation_tokens,
            run_result.usage.cache_read_tokens,
        );
    }

    shutdown(&engine, &mcp_managers).await;
    Ok(())
}

fn emit_resume_banner(output: &dyn OutputSink, session: &aion_agent::session::Session, fork_session: bool) {
    output.emit_info(&resume_banner(session, fork_session));
}

fn resume_banner(session: &aion_agent::session::Session, fork_session: bool) -> String {
    if fork_session {
        format!(
            "Forked session {} from {} ({} messages, {} model)",
            session.id,
            session.forked_from.as_deref().unwrap_or("?"),
            session.messages.len(),
            session.model
        )
    } else {
        format!(
            "Resumed session {} ({} messages, {} model)",
            session.id,
            session.messages.len(),
            session.model
        )
    }
}

async fn shutdown(engine: &AgentEngine, managers: &[Arc<McpManager>]) {
    engine.run_stop_hooks().await;
    for manager in managers {
        manager.shutdown().await;
    }
}

async fn repl_loop(
    engine: &mut AgentEngine,
    terminal: &Arc<TerminalSink>,
    output: &Arc<dyn OutputSink>,
) -> anyhow::Result<()> {
    use std::io::{self, BufRead};

    loop {
        terminal.formatter().repl_prompt();

        let mut input = String::new();
        io::stdin().lock().read_line(&mut input)?;
        let input = input.trim();

        if input.is_empty() {
            break;
        }

        match engine.run(input, "").await {
            Ok(result) => {
                if result.turns > 0 {
                    output.emit_stream_end(
                        "",
                        result.turns,
                        result.usage.input_tokens,
                        result.usage.output_tokens,
                        result.usage.cache_creation_tokens,
                        result.usage.cache_read_tokens,
                    );
                }
            }
            Err(AgentError::UserAborted) => break,
            Err(e) => {
                output.emit_error(&e.to_string());
            }
        }
    }

    Ok(())
}
