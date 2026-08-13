use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use aion_agent::commands::CommandSpec;
use aion_agent::engine::AgentEngine;
use aion_agent::error::AgentError;
use aion_agent::output::OutputSink;
use aion_protocol::commands::ApprovalScope;
use aion_protocol::writer::ProtocolEmitter;
use aion_protocol::{ToolApprovalManager, ToolApprovalResult};
use aion_types::message::Message;
use crossterm::event::{Event, EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use futures::StreamExt;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::time;

use crate::event::{AgentEvent, TuiProtocolEmitter, TuiSink};
use crate::state::AppState;
use crate::terminal::{AppTerminal, TerminalSession};
use crate::ui;

static MESSAGE_SEQUENCE: AtomicU64 = AtomicU64::new(1);

pub struct TuiMetadata {
    pub model: String,
    pub provider: String,
    pub cwd: String,
    pub no_color: bool,
}

pub struct TuiRuntime {
    state: AppState,
    tx: UnboundedSender<AgentEvent>,
    rx: UnboundedReceiver<AgentEvent>,
    approval_manager: Arc<ToolApprovalManager>,
    protocol_emitter: Arc<dyn ProtocolEmitter>,
}

impl TuiRuntime {
    pub fn new(metadata: TuiMetadata) -> Self {
        let (tx, rx) = unbounded_channel();
        let protocol_emitter = TuiProtocolEmitter::shared(tx.clone());
        Self {
            state: AppState::new(metadata.model, metadata.provider, metadata.cwd, metadata.no_color),
            tx,
            rx,
            approval_manager: Arc::new(ToolApprovalManager::new()),
            protocol_emitter,
        }
    }

    pub fn output_sink(&self) -> Arc<dyn OutputSink> {
        TuiSink::shared(self.tx.clone())
    }

    pub fn set_commands(&mut self, commands: Vec<CommandSpec>) {
        self.state.set_commands(commands);
    }

    pub fn set_session_id(&mut self, session_id: Option<String>) {
        self.state.session_id = session_id;
    }

    pub fn set_history(&mut self, messages: &[Message]) {
        self.state.set_history(messages);
    }

    pub async fn run(&mut self, engine: &mut AgentEngine) -> anyhow::Result<()> {
        engine.set_approval_manager(self.approval_manager.clone());
        engine.set_protocol_writer(self.protocol_emitter.clone());

        let mut terminal_session = TerminalSession::enter()?;
        let mut terminal_events = EventStream::new();
        let mut ticker = time::interval(Duration::from_millis(120));
        ticker.set_missed_tick_behavior(time::MissedTickBehavior::Skip);

        loop {
            self.draw(terminal_session.terminal())?;
            tokio::select! {
                event = self.rx.recv() => {
                    if let Some(event) = event {
                        self.state.handle_agent_event(event);
                    }
                }
                event = terminal_events.next() => {
                    let Some(event) = event else {
                        return Ok(());
                    };
                    match event? {
                        Event::Key(key) if is_key_press(key) => {
                            match self.handle_idle_key(key) {
                                IdleAction::Continue => {}
                                IdleAction::Exit => return Ok(()),
                                IdleAction::Submit(input) => {
                                    if self.run_turn(engine, input, terminal_session.terminal(), &mut terminal_events, &mut ticker).await? {
                                        return Ok(());
                                    }
                                }
                            }
                        }
                        Event::Paste(text) => {
                            self.state.composer.insert_text(&text);
                            self.update_popup();
                        }
                        Event::Resize(_, _) => {}
                        _ => {}
                    }
                }
            }
        }
    }

    async fn run_turn(
        &mut self,
        engine: &mut AgentEngine,
        input: String,
        terminal: &mut AppTerminal,
        terminal_events: &mut EventStream,
        ticker: &mut time::Interval,
    ) -> anyhow::Result<bool> {
        self.state.begin_turn(&input);
        let message_id = format!("tui-{}", MESSAGE_SEQUENCE.fetch_add(1, Ordering::Relaxed));
        let mut cancelled = false;
        let result = {
            let engine_future = engine.run(&input, &message_id);
            tokio::pin!(engine_future);

            loop {
                self.draw(terminal)?;
                tokio::select! {
                    result = &mut engine_future => break Some(result),
                    event = self.rx.recv() => {
                        if let Some(event) = event {
                            self.state.handle_agent_event(event);
                        }
                    }
                    event = terminal_events.next() => {
                        let Some(event) = event else {
                            cancelled = true;
                            break None;
                        };
                        match event? {
                            Event::Key(key)
                                if is_key_press(key) && self.handle_running_key(key) =>
                            {
                                cancelled = true;
                                break None;
                            }
                            Event::Resize(_, _) => {}
                            _ => {}
                        }
                    }
                    _ = ticker.tick() => self.state.tick(),
                }
            }
        };

        if cancelled {
            self.deny_pending_approval("Turn cancelled");
            engine.abort_current_turn("Turn cancelled by user");
            self.state.cancel_turn();
            self.drain_agent_events();
            return Ok(false);
        }

        self.drain_agent_events();
        match result {
            Some(Ok(result)) => {
                self.state.finish_turn(result.turns, result.usage);
                Ok(false)
            }
            Some(Err(AgentError::UserAborted)) => {
                self.state.busy = false;
                Ok(true)
            }
            Some(Err(error)) => {
                self.state.busy = false;
                self.state.handle_agent_event(AgentEvent::Error(error.to_string()));
                Ok(false)
            }
            None => Ok(false),
        }
    }

    fn handle_idle_key(&mut self, key: KeyEvent) -> IdleAction {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            if self.state.composer.is_empty() {
                return IdleAction::Exit;
            }
            self.state.composer.clear();
            self.update_popup();
            return IdleAction::Continue;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && key.code == KeyCode::Char('d')
            && self.state.composer.is_empty()
        {
            return IdleAction::Exit;
        }

        let popup_visible = self.state.popup.is_visible(&self.state.composer.text());
        match key.code {
            KeyCode::PageUp => self.state.scroll_back = self.state.scroll_back.saturating_add(5),
            KeyCode::PageDown => self.state.scroll_back = self.state.scroll_back.saturating_sub(5),
            KeyCode::Up if popup_visible => self.state.popup.move_previous(),
            KeyCode::Down if popup_visible => self.state.popup.move_next(),
            KeyCode::Tab if popup_visible => {
                if let Some(name) = self.state.popup.selected_name() {
                    self.state.composer.replace_command(&name);
                    self.update_popup();
                }
            }
            KeyCode::Enter if !key.modifiers.contains(KeyModifiers::SHIFT) => {
                if popup_visible && let Some(name) = self.state.popup.selected_name() {
                    self.state.composer.replace_command(&name);
                }
                let input = self.state.composer.take();
                self.update_popup();
                if !input.trim().is_empty() {
                    return IdleAction::Submit(input);
                }
            }
            KeyCode::Esc if popup_visible => {
                self.state.composer.clear();
                self.update_popup();
            }
            _ => {
                if self.state.composer.input(key) {
                    self.update_popup();
                }
            }
        }
        IdleAction::Continue
    }

    /// Returns true when the active turn should be cancelled.
    fn handle_running_key(&mut self, key: KeyEvent) -> bool {
        if let Some(request) = &self.state.approval {
            let call_id = request.call_id.clone();
            match key.code {
                KeyCode::Char('y') | KeyCode::Enter => {
                    self.approval_manager.approve(&call_id, ApprovalScope::Once);
                    self.state.approval = None;
                }
                KeyCode::Char('a') => {
                    self.approval_manager.approve(&call_id, ApprovalScope::Always);
                    self.state.approval = None;
                }
                KeyCode::Char('n') | KeyCode::Esc => {
                    self.approval_manager.resolve(
                        &call_id,
                        ToolApprovalResult::Denied {
                            reason: "Denied by user".to_string(),
                        },
                    );
                    self.state.approval = None;
                }
                _ => {}
            }
            return false;
        }

        key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c')
    }

    fn deny_pending_approval(&mut self, reason: &str) {
        let Some(request) = self.state.approval.take() else {
            return;
        };
        self.approval_manager.resolve(
            &request.call_id,
            ToolApprovalResult::Denied {
                reason: reason.to_string(),
            },
        );
    }

    fn update_popup(&mut self) {
        let text = self.state.composer.text();
        self.state.popup.update(&text);
    }

    fn drain_agent_events(&mut self) {
        while let Ok(event) = self.rx.try_recv() {
            self.state.handle_agent_event(event);
        }
    }

    fn draw(&mut self, terminal: &mut AppTerminal) -> anyhow::Result<()> {
        terminal.draw(|frame| ui::render(frame, &self.state))?;
        Ok(())
    }
}

fn is_key_press(key: KeyEvent) -> bool {
    matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat)
}

enum IdleAction {
    Continue,
    Submit(String),
    Exit,
}
