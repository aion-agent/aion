use futures_util::StreamExt;
use std::io;
use std::path::Path;
use tokio::sync::mpsc::unbounded_channel;

use crossterm::{
    event::{Event, EventStream, KeyCode, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Tabs, Wrap},
};

use crate::agent_direct::Agent;

enum TuiEvent {
    Token(String),
    Finished(anyhow::Result<String>),
}

struct TuiState {
    tab_index: usize,
    messages: Vec<(String, String)>, // (role, content)
    input_buffer: String,
    input_focused: bool,
    agent_running: bool,
    integrations_index: usize,
    settings_index: usize,
    editing_setting: Option<usize>,
    editing_buffer: String,
    chat_scroll: usize,
}

const SETTINGS_FIELDS: &[&str] = &[
    "Base URL",
    "Model Name",
    "Temperature",
    "Max History Messages",
    "Max Tool Errors",
    "OpenRouter Model",
    "OpenRouter API Key",
];

fn get_setting_value(agent: Option<&Agent>, field: &str) -> String {
    let Some(agent) = agent else {
        return "".to_string();
    };
    match field {
        "Base URL" => agent.config.base_url.clone(),
        "Model Name" => agent.config.model.clone(),
        "Temperature" => agent.config.temperature.to_string(),
        "Max History Messages" => agent.config.max_history_messages.to_string(),
        "Max Tool Errors" => agent.config.max_tool_errors.to_string(),
        "OpenRouter Model" => agent.config.openrouter_model.clone().unwrap_or_default(),
        "OpenRouter API Key" => agent.config.openrouter_api_key.clone().unwrap_or_default(),
        _ => "".to_string(),
    }
}

fn set_setting_value(agent: &mut Agent, field: &str, value: &str) {
    match field {
        "Base URL" => agent.config.base_url = value.to_string(),
        "Model Name" => agent.config.model = value.to_string(),
        "Temperature" => {
            if let Ok(f) = value.parse() {
                agent.config.temperature = f;
            }
        }
        "Max History Messages" => {
            if let Ok(n) = value.parse() {
                agent.config.max_history_messages = n;
            }
        }
        "Max Tool Errors" => {
            if let Ok(n) = value.parse() {
                agent.config.max_tool_errors = n;
            }
        }
        "OpenRouter Model" => {
            agent.config.openrouter_model = if value.trim().is_empty() {
                None
            } else {
                Some(value.to_string())
            };
        }
        "OpenRouter API Key" => {
            agent.config.openrouter_api_key = if value.trim().is_empty() {
                None
            } else {
                Some(value.to_string())
            };
        }
        _ => {}
    }
}

fn update_config_file(field: &str, value: &str) -> anyhow::Result<()> {
    let path = Path::new("root.toml");
    if !path.exists() {
        return Ok(());
    }
    let raw = std::fs::read_to_string(path)?;
    let mut toml_val: toml::Value = toml::from_str(&raw)?;
    if let Some(agent) = toml_val.get_mut("agent") {
        if let Some(table) = agent.as_table_mut() {
            match field {
                "Base URL" => {
                    table.insert(
                        "base_url".to_string(),
                        toml::Value::String(value.to_string()),
                    );
                }
                "Model Name" => {
                    table.insert("model".to_string(), toml::Value::String(value.to_string()));
                }
                "OpenRouter Model" => {
                    if value.trim().is_empty() {
                        table.remove("openrouter_model");
                    } else {
                        table.insert(
                            "openrouter_model".to_string(),
                            toml::Value::String(value.to_string()),
                        );
                    }
                }
                "OpenRouter API Key" => {
                    if value.trim().is_empty() {
                        table.remove("openrouter_api_key");
                    } else {
                        table.insert(
                            "openrouter_api_key".to_string(),
                            toml::Value::String(value.to_string()),
                        );
                    }
                }
                "Max Tool Errors" => {
                    if let Ok(num) = value.parse::<i64>() {
                        table.insert("max_tool_errors".to_string(), toml::Value::Integer(num));
                    }
                }
                "Max History Messages" => {
                    if let Ok(num) = value.parse::<i64>() {
                        table.insert(
                            "max_history_messages".to_string(),
                            toml::Value::Integer(num),
                        );
                    }
                }
                "Temperature" => {
                    if let Ok(f) = value.parse::<f64>() {
                        table.insert("temperature".to_string(), toml::Value::Float(f));
                    }
                }
                _ => {}
            }
        }
    }
    std::fs::write(path, toml::to_string_pretty(&toml_val)?)?;
    Ok(())
}

fn toggle_integration_file(name: &str, enabled: bool) -> anyhow::Result<()> {
    let path_str = format!("./integrations/{}.toml", name);
    let path = Path::new(&path_str);
    if !path.exists() {
        return Ok(());
    }
    let raw = std::fs::read_to_string(path)?;
    let mut value: toml::Value = toml::from_str(&raw)?;
    if let Some(integration) = value.get_mut("integration") {
        if let Some(enabled_val) = integration.get_mut("enabled") {
            *enabled_val = toml::Value::Boolean(enabled);
        } else if let Some(table) = integration.as_table_mut() {
            table.insert("enabled".to_string(), toml::Value::Boolean(enabled));
        }
    }
    std::fs::write(path, toml::to_string_pretty(&value)?)?;
    Ok(())
}

pub async fn run_tui(agent: Agent) -> anyhow::Result<()> {
    let local = tokio::task::LocalSet::new();
    local.run_until(run_tui_inner(agent)).await
}

async fn run_tui_inner(agent: Agent) -> anyhow::Result<()> {
    use std::cell::RefCell;
    use std::rc::Rc;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut messages = Vec::new();
    for msg in &agent.history {
        let role = msg["role"].as_str().unwrap_or("").to_string();
        let content = msg["content"].as_str().unwrap_or("").to_string();
        if role == "user" || role == "assistant" {
            messages.push((role, content));
        }
    }

    // Wrap Agent in Option so we can take() it during the async step()
    let agent = Rc::new(RefCell::new(Some(agent)));

    let mut state = TuiState {
        tab_index: 0,
        messages,
        input_buffer: String::new(),
        input_focused: true,
        agent_running: false,
        integrations_index: 0,
        settings_index: 0,
        editing_setting: None,
        editing_buffer: String::new(),
        chat_scroll: 0,
    };

    let (tx_event, mut rx_event) = unbounded_channel::<TuiEvent>();
    let (tx_agent_return, mut rx_agent_return) = unbounded_channel::<Agent>();
    let mut events = EventStream::new();

    loop {
        // Render — agent may be None while running
        let agent_ref = agent.borrow();
        terminal.draw(|f| draw_ui(f, &state, agent_ref.as_ref()))?;

        tokio::select! {
            Some(tui_event) = rx_event.recv() => {
                match tui_event {
                    TuiEvent::Token(token) => {
                        if let Some(last_msg) = state.messages.last_mut() {
                            if last_msg.0 == "assistant" {
                                last_msg.1.push_str(&token);
                            }
                        }
                    }
                    TuiEvent::Finished(result) => {
                        state.agent_running = false;
                        if let Err(e) = result {
                            state.messages.push(("assistant".to_string(), format!("System Error: {}", e)));
                        }
                    }
                }
            }
            // Receive Agent back when step() finishes
            Some(agent_back) = rx_agent_return.recv() => {
                *agent.borrow_mut() = Some(agent_back);
            }
            Some(Ok(event)) = events.next() => {
                if let Event::Key(key) = event {
                    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                        break;
                    }

                    if state.editing_setting.is_some() {
                        match key.code {
                            KeyCode::Enter => {
                                if let Some(idx) = state.editing_setting {
                                    let field = SETTINGS_FIELDS[idx];
                                    let val = state.editing_buffer.clone();
                                    if let Some(ref mut a) = *agent.borrow_mut() {
                                        set_setting_value(a, field, &val);
                                    }
                                    let _ = update_config_file(field, &val);
                                }
                                state.editing_setting = None;
                            }
                            KeyCode::Esc => { state.editing_setting = None; }
                            KeyCode::Char(c) => { state.editing_buffer.push(c); }
                            KeyCode::Backspace => { state.editing_buffer.pop(); }
                            _ => {}
                        }
                    } else if state.input_focused && state.tab_index == 0 {
                        match key.code {
                            KeyCode::Enter => {
                                if !state.input_buffer.trim().is_empty() && !state.agent_running {
                                    let prompt = state.input_buffer.clone();
                                    state.input_buffer.clear();
                                    state.messages.push(("user".to_string(), prompt.clone()));
                                    state.messages.push(("assistant".to_string(), String::new()));
                                    state.agent_running = true;

                                    let tx_clone = tx_event.clone();
                                    let (tx_token, mut rx_token) = unbounded_channel::<String>();

                                    // Forward tokens from agent channel to TUI event channel
                                    let tx_fwd = tx_clone.clone();
                                    tokio::task::spawn_local(async move {
                                        while let Some(tok) = rx_token.recv().await {
                                            let _ = tx_fwd.send(TuiEvent::Token(tok));
                                        }
                                    });

                                    // Run agent step on LocalSet — take() the Agent out so RefCell is free
                                    let tx_agent_return_clone = tx_agent_return.clone();
                                    let agent_rc = Rc::clone(&agent);
                                    tokio::task::spawn_local(async move {
                                        let mut agent_opt = agent_rc.borrow_mut().take().unwrap();
                                        let result = agent_opt.step(&prompt, tx_token).await;
                                        let _ = tx_clone.send(TuiEvent::Finished(result));
                                        let _ = tx_agent_return_clone.send(agent_opt);
                                    });
                                }
                            }
                            KeyCode::Esc => { state.input_focused = false; }
                            KeyCode::Char(c) => { state.input_buffer.push(c); }
                            KeyCode::Backspace => { state.input_buffer.pop(); }
                            _ => {}
                        }
                    } else {
                        match key.code {
                            KeyCode::Tab | KeyCode::Right => { state.tab_index = (state.tab_index + 1) % 3; }
                            KeyCode::Left => { state.tab_index = (state.tab_index + 2) % 3; }
                            KeyCode::Char('1') => state.tab_index = 0,
                            KeyCode::Char('2') => state.tab_index = 1,
                            KeyCode::Char('3') => state.tab_index = 2,
                            KeyCode::Char('i') => { if state.tab_index == 0 { state.input_focused = true; } }
                            KeyCode::Up => {
                                match state.tab_index {
                                    0 => { if state.chat_scroll > 0 { state.chat_scroll -= 1; } }
                                    1 => { if state.integrations_index > 0 { state.integrations_index -= 1; } }
                                    2 => { if state.settings_index > 0 { state.settings_index -= 1; } }
                                    _ => {}
                                }
                            }
                            KeyCode::Down => {
                                match state.tab_index {
                                    0 => { state.chat_scroll += 1; }
                                    1 => {
                                        let len = agent.borrow().as_ref().map(|a| a.integrations.len()).unwrap_or(0);
                                        if state.integrations_index + 1 < len {
                                            state.integrations_index += 1;
                                        }
                                    }
                                    2 => { if state.settings_index + 1 < SETTINGS_FIELDS.len() { state.settings_index += 1; } }
                                    _ => {}
                                }
                            }
                            KeyCode::Char(' ') | KeyCode::Enter => {
                                if state.tab_index == 1 {
                                    let mut name = None;
                                    let mut new_enabled = false;
                                    if let Some(ref a) = *agent.borrow() {
                                        if !a.integrations.is_empty() {
                                            name = Some(a.integrations[state.integrations_index].name.clone());
                                            new_enabled = !a.integrations[state.integrations_index].enabled;
                                        }
                                    }
                                    if let Some(name) = name {
                                        if let Some(ref mut a) = *agent.borrow_mut() {
                                            a.integrations[state.integrations_index].enabled = new_enabled;
                                        }
                                        let _ = toggle_integration_file(&name, new_enabled);
                                    }
                                } else if state.tab_index == 2 {
                                    state.editing_setting = Some(state.settings_index);
                                    state.editing_buffer = get_setting_value(agent.borrow().as_ref(), SETTINGS_FIELDS[state.settings_index]);
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    Ok(())
}

fn draw_ui(f: &mut ratatui::Frame, state: &TuiState, agent: Option<&Agent>) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Tab bar
            Constraint::Min(0),    // Main area
            Constraint::Length(1), // Footer status bar
        ])
        .split(f.size());

    // Render Tabs
    let tab_titles = vec![
        Span::styled(" 💬 Chat [1] ", Style::default().fg(Color::White)),
        Span::styled(" 🔌 Integrations [2] ", Style::default().fg(Color::White)),
        Span::styled(" ⚙ Settings [3] ", Style::default().fg(Color::White)),
    ];
    let tabs = Tabs::new(tab_titles)
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .select(state.tab_index)
        .style(Style::default().fg(Color::Gray))
        .highlight_style(
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        );
    f.render_widget(tabs, chunks[0]);

    // Render Active Tab Content
    match state.tab_index {
        0 => draw_chat_tab(f, chunks[1], state, agent),
        1 => draw_integrations_tab(f, chunks[1], state, agent),
        2 => draw_settings_tab(f, chunks[1], state, agent),
        _ => {}
    }

    // Render status bar footer
    let active_scope_name = agent
        .and_then(|a| a.active_memory.as_ref().map(|(name, _)| name.as_str()))
        .unwrap_or("root");
    let base_url = agent.map(|a| a.config.base_url.as_str()).unwrap_or("...");
    let status_text = format!(
        " Active Scope: {} | API: {} | Ctrl+C to Exit",
        active_scope_name.to_uppercase(),
        base_url
    );
    let status_bar = Paragraph::new(Span::styled(
        status_text,
        Style::default().fg(Color::DarkGray),
    ))
    .style(Style::default().bg(Color::Rgb(20, 20, 20)));
    f.render_widget(status_bar, chunks[2]);
}

fn draw_chat_tab(f: &mut ratatui::Frame, rect: Rect, state: &TuiState, agent: Option<&Agent>) {
    let main_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(75), // Left Chat pane
            Constraint::Percentage(25), // Right Sidebar pane
        ])
        .split(rect);

    // Left Area: Chat Log + Input Box
    let chat_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),    // Chat log history
            Constraint::Length(3), // Input box
        ])
        .split(main_layout[0]);

    // Build Chat History log text
    let mut chat_spans = Vec::new();
    for (role, content) in &state.messages {
        let prefix = if role == "user" {
            Span::styled(
                "user> ",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            Span::styled(
                "aion> ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
        };
        chat_spans.push(Line::from(vec![prefix, Span::raw(content)]));
        chat_spans.push(Line::from("")); // Spacing
    }

    // Simple scroll calculation to always show latest messages if active_scrolled
    let total_lines = chat_spans.len();
    let display_height = chat_layout[0].height as usize - 2;
    let scroll = if total_lines > display_height {
        if state.input_focused {
            total_lines - display_height
        } else {
            state.chat_scroll.min(total_lines - display_height)
        }
    } else {
        0
    };

    let chat_block = Block::default()
        .title(" Conversation History ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if state.input_focused {
            Color::DarkGray
        } else {
            Color::Cyan
        }));

    let chat_paragraph = Paragraph::new(chat_spans)
        .block(chat_block)
        .wrap(Wrap { trim: true })
        .scroll((scroll as u16, 0));
    f.render_widget(chat_paragraph, chat_layout[0]);

    // Render Input Box widget
    let border_color = if state.input_focused {
        Color::Green
    } else {
        Color::DarkGray
    };
    let input_title = if state.agent_running {
        " Agent is thinking... "
    } else if state.input_focused {
        " Input Prompt (Press Esc to Scroll History) "
    } else {
        " Normal Mode (Press 'i' or Enter to write message) "
    };

    let input_block = Block::default()
        .title(input_title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));
    let input_para = Paragraph::new(state.input_buffer.as_str())
        .block(input_block)
        .wrap(Wrap { trim: true });
    f.render_widget(input_para, chat_layout[1]);

    // Right Area: Active Scope information and Integrations status sidebar
    let sidebar_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6), // Scope info box
            Constraint::Min(0),    // Integrations listing
        ])
        .split(main_layout[1]);

    // Scope Block
    let active_scope_name = agent
        .and_then(|a| a.active_memory.as_ref().map(|(name, _)| name.as_str()))
        .unwrap_or("root");
    let scope_details = vec![
        Line::from(vec![
            Span::raw("Scope: "),
            Span::styled(
                active_scope_name.to_uppercase(),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(format!(
            "Status: {}",
            if active_scope_name == "root" {
                "Global DB"
            } else {
                "Isolated child DB"
            }
        )),
    ];
    let scope_block = Block::default()
        .title(" Memory Scope ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let scope_para = Paragraph::new(scope_details).block(scope_block);
    f.render_widget(scope_para, sidebar_layout[0]);

    // Sidebar Integrations List
    let mut list_items = Vec::new();
    if let Some(a) = agent {
        for i in &a.integrations {
            let status = if i.enabled {
                Span::styled(" [ON] ", Style::default().fg(Color::Green))
            } else {
                Span::styled(" [OFF]", Style::default().fg(Color::Red))
            };
            list_items.push(Line::from(vec![status, Span::raw(format!(" {}", i.name))]));
        }
    }
    let list_block = Block::default()
        .title(" Integrations ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let list_para = Paragraph::new(list_items).block(list_block);
    f.render_widget(list_para, sidebar_layout[1]);
}

fn draw_integrations_tab(f: &mut ratatui::Frame, rect: Rect, state: &TuiState, agent: Option<&Agent>) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(40), // Left list
            Constraint::Percentage(60), // Right details
        ])
        .split(rect);

    // Left List Pane
    let mut list_items = Vec::new();
    if let Some(a) = agent {
        for (i, integration) in a.integrations.iter().enumerate() {
            let is_selected = i == state.integrations_index;
            let prefix = if is_selected { "> " } else { "  " };
            let status = if integration.enabled {
                "[Enabled] "
            } else {
                "[Disabled]"
            };
            let style = if is_selected {
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            list_items.push(Line::from(vec![
                Span::styled(format!("{}{}", prefix, status), style),
                Span::raw(format!(" - {}", integration.name)),
            ]));
        }
    }

    let list_block = Block::default()
        .title(" Loaded Integrations (Press Space to Toggle) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let list_paragraph = Paragraph::new(list_items).block(list_block);
    f.render_widget(list_paragraph, chunks[0]);

    // Right Details Pane
    let mut details_spans = Vec::new();
    if let Some(a) = agent {
        if !a.integrations.is_empty() {
            let integration = &a.integrations[state.integrations_index];
            details_spans.push(Line::from(vec![
                Span::styled("Name: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(&integration.name, Style::default().fg(Color::Cyan)),
            ]));
            details_spans.push(Line::from(vec![
                Span::styled(
                    "Description: ",
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::raw(&integration.description),
            ]));
            details_spans.push(Line::from(vec![
                Span::styled(
                    "Executor command: ",
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::styled(&integration.executor, Style::default().fg(Color::Yellow)),
            ]));
            details_spans.push(Line::from(""));
            details_spans.push(Line::from(Span::styled(
                "Provided Tools:",
                Style::default().add_modifier(Modifier::BOLD),
            )));
            for tool in &integration.tools {
                details_spans.push(Line::from(format!(
                    " - {}: {}",
                    tool.name, tool.description
                )));
            }
        } else {
            details_spans.push(Line::from(
                "No integrations loaded. Check `./integrations/` directory.",
            ));
        }
    } else {
        details_spans.push(Line::from(
            "Agent busy...",
        ));
    }

    let details_block = Block::default()
        .title(" Integration Details ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let details_paragraph = Paragraph::new(details_spans)
        .block(details_block)
        .wrap(Wrap { trim: true });
    f.render_widget(details_paragraph, chunks[1]);
}

fn draw_settings_tab(f: &mut ratatui::Frame, rect: Rect, state: &TuiState, agent: Option<&Agent>) {
    let mut list_items = Vec::new();
    for (i, field) in SETTINGS_FIELDS.iter().enumerate() {
        let is_selected = i == state.settings_index;
        let prefix = if is_selected { "> " } else { "  " };
        let value = get_setting_value(agent, field);

        let style = if is_selected {
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        list_items.push(Line::from(vec![
            Span::styled(format!("{}{}: ", prefix, field), style),
            Span::styled(value, Style::default().fg(Color::Cyan)),
        ]));
    }

    let settings_block = Block::default()
        .title(" Configuration parameters (Press Enter to edit field) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let settings_paragraph = Paragraph::new(list_items).block(settings_block);
    f.render_widget(settings_paragraph, rect);

    // If inline editing setting field, render popup box
    if let Some(idx) = state.editing_setting {
        let field = SETTINGS_FIELDS[idx];
        let area = centered_rect(60, 20, rect);
        f.render_widget(Clear, area); // clear background

        let edit_block = Block::default()
            .title(format!(
                " Edit field: {} (Press Enter to save, Esc to cancel) ",
                field
            ))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow));
        let edit_paragraph = Paragraph::new(state.editing_buffer.as_str())
            .block(edit_block)
            .wrap(Wrap { trim: true });
        f.render_widget(edit_paragraph, area);
    }
}

/// Helper to render centered popup layout
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

