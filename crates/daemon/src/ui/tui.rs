//! Terminal User Interface for the RemoShell daemon.
//!
//! This module provides a ratatui-based TUI for monitoring and managing
//! the daemon, including connected devices, active sessions, and pending
//! approval requests.

use std::io::{self, Stdout};
use std::net::IpAddr;
use std::time::{Duration, Instant};

use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyModifiers,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Tabs},
    Frame, Terminal,
};
use tokio::sync::mpsc;

/// Action to take on an approval request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalAction {
    /// Accept the device connection.
    Accept,
    /// Reject the device connection.
    Reject,
}

/// Result of an approval action from the TUI.
#[derive(Debug, Clone)]
pub struct ApprovalResult {
    /// The device ID that was acted upon.
    pub device_id: String,
    /// The action taken.
    pub action: ApprovalAction,
    /// Whether to always trust this device (add to trust store).
    pub always_trust: bool,
    /// Device name for trust store entry.
    pub device_name: String,
    /// Public key for trust store entry.
    pub public_key: Option<[u8; 32]>,
}

/// Events that can be sent to the TUI from the daemon.
#[derive(Debug, Clone)]
pub enum TuiEvent {
    /// A new device connected.
    DeviceConnected { device_id: String, name: String },
    /// A device disconnected.
    DeviceDisconnected { device_id: String },
    /// A new session was created.
    SessionCreated {
        session_id: String,
        device_id: String,
    },
    /// A session was terminated.
    SessionTerminated { session_id: String },
    /// A device is requesting approval.
    ApprovalRequested {
        device_id: String,
        name: String,
        ip_address: Option<IpAddr>,
        public_key: Option<[u8; 32]>,
    },
    /// An approval request was handled.
    ApprovalHandled { device_id: String, approved: bool },
    /// Update statistics.
    StatsUpdate {
        devices_connected: usize,
        sessions_active: usize,
        uptime_secs: u64,
    },
    /// Force a UI refresh.
    Refresh,
    /// Shutdown the TUI.
    Shutdown,
}

/// The available tabs in the TUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    /// Status overview tab.
    #[default]
    Status,
    /// Active sessions tab.
    Sessions,
    /// Connected devices tab.
    Devices,
    /// Pending approval requests tab.
    Approvals,
}

impl Tab {
    /// Returns the index of the tab.
    pub fn index(&self) -> usize {
        match self {
            Tab::Status => 0,
            Tab::Sessions => 1,
            Tab::Devices => 2,
            Tab::Approvals => 3,
        }
    }

    /// Returns the tab from an index.
    pub fn from_index(index: usize) -> Self {
        match index {
            0 => Tab::Status,
            1 => Tab::Sessions,
            2 => Tab::Devices,
            3 => Tab::Approvals,
            _ => Tab::Status,
        }
    }

    /// Returns all tabs.
    pub fn all() -> &'static [Tab] {
        &[Tab::Status, Tab::Sessions, Tab::Devices, Tab::Approvals]
    }

    /// Returns the title of the tab.
    pub fn title(&self) -> &'static str {
        match self {
            Tab::Status => "Status",
            Tab::Sessions => "Sessions",
            Tab::Devices => "Devices",
            Tab::Approvals => "Approvals",
        }
    }

    /// Returns the next tab.
    pub fn next(&self) -> Self {
        let idx = (self.index() + 1) % 4;
        Tab::from_index(idx)
    }

    /// Returns the previous tab.
    pub fn prev(&self) -> Self {
        let idx = if self.index() == 0 {
            3
        } else {
            self.index() - 1
        };
        Tab::from_index(idx)
    }
}

/// Information about a connected device for display.
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    /// Device ID.
    pub id: String,
    /// Device name.
    pub name: String,
    /// Whether the device is connected.
    pub connected: bool,
}

/// Information about an active session for display.
#[derive(Debug, Clone)]
pub struct SessionInfo {
    /// Session ID.
    pub id: String,
    /// Device ID that owns this session.
    pub device_id: String,
    /// Terminal size (cols x rows).
    pub size: (u16, u16),
}

/// Information about a pending approval request.
#[derive(Debug, Clone)]
pub struct ApprovalInfo {
    /// Device ID requesting approval.
    pub device_id: String,
    /// Device name.
    pub name: String,
    /// IP address of the requesting device.
    pub ip_address: Option<IpAddr>,
    /// Public key of the requesting device (32 bytes, Ed25519).
    pub public_key: Option<[u8; 32]>,
    /// When the request was received.
    pub requested_at: Instant,
}

/// Statistics for the status bar.
#[derive(Debug, Clone, Default)]
pub struct DaemonStats {
    /// Number of connected devices.
    pub devices_connected: usize,
    /// Number of active sessions.
    pub sessions_active: usize,
    /// Daemon uptime in seconds.
    pub uptime_secs: u64,
}

/// The main TUI application.
pub struct TuiApp {
    /// The terminal backend.
    terminal: Terminal<CrosstermBackend<Stdout>>,
    /// The currently selected tab.
    current_tab: Tab,
    /// Whether the TUI should quit.
    should_quit: bool,
    /// Receiver for daemon events.
    event_rx: mpsc::Receiver<TuiEvent>,
    /// Sender for daemon events (kept for cloning).
    event_tx: mpsc::Sender<TuiEvent>,
    /// List of connected devices.
    devices: Vec<DeviceInfo>,
    /// List of active sessions.
    sessions: Vec<SessionInfo>,
    /// List of pending approvals.
    approvals: Vec<ApprovalInfo>,
    /// Daemon statistics.
    stats: DaemonStats,
    /// When the TUI was started.
    start_time: Instant,
    /// Selected index in the current list.
    list_state: ListState,
    /// Whether to always trust the device when approving.
    always_trust: bool,
    /// Channel for sending approval results to the daemon.
    approval_tx: mpsc::Sender<ApprovalResult>,
    /// Receiver for approval results (kept for handing off to daemon).
    approval_rx: Option<mpsc::Receiver<ApprovalResult>>,
}

impl TuiApp {
    /// Creates a new TUI application.
    ///
    /// Returns the TuiApp and a sender for sending events to it.
    pub fn new() -> io::Result<(Self, mpsc::Sender<TuiEvent>)> {
        // Set up terminal
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;

        // Create event channel
        let (event_tx, event_rx) = mpsc::channel(100);
        let tx_clone = event_tx.clone();

        // Create approval result channel
        let (approval_tx, approval_rx) = mpsc::channel(100);

        let mut list_state = ListState::default();
        list_state.select(Some(0));

        Ok((
            Self {
                terminal,
                current_tab: Tab::Status,
                should_quit: false,
                event_rx,
                event_tx,
                devices: Vec::new(),
                sessions: Vec::new(),
                approvals: Vec::new(),
                stats: DaemonStats::default(),
                start_time: Instant::now(),
                list_state,
                always_trust: false,
                approval_tx,
                approval_rx: Some(approval_rx),
            },
            tx_clone,
        ))
    }

    /// Creates a TUI app for testing (without terminal setup).
    #[cfg(test)]
    pub fn new_for_testing() -> io::Result<(Self, mpsc::Sender<TuiEvent>)> {
        let (event_tx, event_rx) = mpsc::channel(100);
        let tx_clone = event_tx.clone();

        // Create approval result channel
        let (approval_tx, approval_rx) = mpsc::channel(100);

        // Create a mock terminal that writes to a buffer
        let stdout = io::stdout();
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;

        let mut list_state = ListState::default();
        list_state.select(Some(0));

        Ok((
            Self {
                terminal,
                current_tab: Tab::Status,
                should_quit: false,
                event_rx,
                event_tx,
                devices: Vec::new(),
                sessions: Vec::new(),
                approvals: Vec::new(),
                stats: DaemonStats::default(),
                start_time: Instant::now(),
                list_state,
                always_trust: false,
                approval_tx,
                approval_rx: Some(approval_rx),
            },
            tx_clone,
        ))
    }

    /// Returns a clone of the event sender.
    pub fn event_sender(&self) -> mpsc::Sender<TuiEvent> {
        self.event_tx.clone()
    }

    /// Takes the approval result receiver.
    ///
    /// This should be called once to get the receiver that the daemon
    /// will use to receive approval results from the TUI.
    /// Returns `None` if the receiver has already been taken.
    pub fn take_approval_receiver(&mut self) -> Option<mpsc::Receiver<ApprovalResult>> {
        self.approval_rx.take()
    }

    /// Returns the current state of the "always trust" toggle.
    pub fn always_trust(&self) -> bool {
        self.always_trust
    }

    /// Toggles the "always trust" option.
    pub fn toggle_always_trust(&mut self) {
        self.always_trust = !self.always_trust;
        tracing::debug!("Always trust toggled to: {}", self.always_trust);
    }

    /// Returns the current tab.
    pub fn current_tab(&self) -> Tab {
        self.current_tab
    }

    /// Sets the current tab.
    pub fn set_tab(&mut self, tab: Tab) {
        self.current_tab = tab;
        self.list_state.select(Some(0));
    }

    /// Moves to the next tab.
    pub fn next_tab(&mut self) {
        self.current_tab = self.current_tab.next();
        self.list_state.select(Some(0));
    }

    /// Moves to the previous tab.
    pub fn prev_tab(&mut self) {
        self.current_tab = self.current_tab.prev();
        self.list_state.select(Some(0));
    }

    /// Moves selection down in the current list.
    pub fn select_next(&mut self) {
        let len = self.current_list_len();
        if len == 0 {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) => {
                if i >= len - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    /// Moves selection up in the current list.
    pub fn select_prev(&mut self) {
        let len = self.current_list_len();
        if len == 0 {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) => {
                if i == 0 {
                    len - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    /// Returns the length of the current list.
    fn current_list_len(&self) -> usize {
        match self.current_tab {
            Tab::Status => 0,
            Tab::Sessions => self.sessions.len(),
            Tab::Devices => self.devices.len(),
            Tab::Approvals => self.approvals.len(),
        }
    }

    /// Handles a keyboard event.
    pub fn handle_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                self.should_quit = true;
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true;
            }
            KeyCode::Tab | KeyCode::Right => {
                self.next_tab();
            }
            KeyCode::BackTab | KeyCode::Left => {
                self.prev_tab();
            }
            KeyCode::Char('1') => self.set_tab(Tab::Status),
            KeyCode::Char('2') => self.set_tab(Tab::Sessions),
            KeyCode::Char('3') => self.set_tab(Tab::Devices),
            KeyCode::Char('4') => self.set_tab(Tab::Approvals),
            KeyCode::Down | KeyCode::Char('j') => {
                self.select_next();
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.select_prev();
            }
            KeyCode::Enter => {
                self.handle_enter();
            }
            // Approval tab specific keys
            KeyCode::Char('a') | KeyCode::Char('A') => {
                if self.current_tab == Tab::Approvals {
                    self.handle_approval_action(ApprovalAction::Accept);
                }
            }
            KeyCode::Char('r') | KeyCode::Char('R') => {
                if self.current_tab == Tab::Approvals {
                    self.handle_approval_action(ApprovalAction::Reject);
                }
            }
            KeyCode::Char('t') | KeyCode::Char('T') => {
                if self.current_tab == Tab::Approvals {
                    self.toggle_always_trust();
                }
            }
            _ => {}
        }
    }

    /// Handles the Enter key action on the current selection.
    fn handle_enter(&mut self) {
        match self.current_tab {
            Tab::Approvals => {
                // Enter key on Approvals tab accepts the device
                self.handle_approval_action(ApprovalAction::Accept);
            }
            Tab::Sessions => {
                // Handle session action (e.g., view details or kill)
                if let Some(idx) = self.list_state.selected() {
                    if idx < self.sessions.len() {
                        tracing::info!("Selected session: {}", self.sessions[idx].id);
                    }
                }
            }
            _ => {}
        }
    }

    /// Handles an approval action (accept or reject) on the selected device.
    fn handle_approval_action(&mut self, action: ApprovalAction) {
        if let Some(idx) = self.list_state.selected() {
            if idx < self.approvals.len() {
                let approval = &self.approvals[idx];
                let result = ApprovalResult {
                    device_id: approval.device_id.clone(),
                    action,
                    always_trust: self.always_trust,
                    device_name: approval.name.clone(),
                    public_key: approval.public_key,
                };

                let action_str = match action {
                    ApprovalAction::Accept => "Accepting",
                    ApprovalAction::Reject => "Rejecting",
                };

                tracing::info!(
                    "{} device: {} (always_trust: {})",
                    action_str,
                    approval.device_id,
                    self.always_trust
                );

                // Send the approval result through the channel
                if let Err(e) = self.approval_tx.try_send(result) {
                    tracing::error!("Failed to send approval result: {}", e);
                } else {
                    // Remove the approval from the list
                    self.approvals.remove(idx);
                    // Adjust selection if needed
                    if !self.approvals.is_empty() {
                        if idx >= self.approvals.len() {
                            self.list_state.select(Some(self.approvals.len() - 1));
                        }
                    } else {
                        self.list_state.select(Some(0));
                    }
                    // Reset always_trust after action
                    self.always_trust = false;
                }
            }
        }
    }

    /// Returns the list of pending approvals.
    pub fn approvals(&self) -> &[ApprovalInfo] {
        &self.approvals
    }

    /// Adds a pending approval request.
    pub fn add_approval(&mut self, approval: ApprovalInfo) {
        self.approvals.push(approval);
    }

    /// Removes a pending approval by device ID.
    pub fn remove_approval(&mut self, device_id: &str) {
        self.approvals.retain(|a| a.device_id != device_id);
    }

    /// Processes a TUI event.
    pub fn handle_event(&mut self, event: TuiEvent) {
        match event {
            TuiEvent::DeviceConnected { device_id, name } => {
                self.devices.push(DeviceInfo {
                    id: device_id,
                    name,
                    connected: true,
                });
            }
            TuiEvent::DeviceDisconnected { device_id } => {
                if let Some(pos) = self.devices.iter().position(|d| d.id == device_id) {
                    self.devices[pos].connected = false;
                }
            }
            TuiEvent::SessionCreated {
                session_id,
                device_id,
            } => {
                self.sessions.push(SessionInfo {
                    id: session_id,
                    device_id,
                    size: (80, 24),
                });
            }
            TuiEvent::SessionTerminated { session_id } => {
                self.sessions.retain(|s| s.id != session_id);
            }
            TuiEvent::ApprovalRequested {
                device_id,
                name,
                ip_address,
                public_key,
            } => {
                self.approvals.push(ApprovalInfo {
                    device_id,
                    name,
                    ip_address,
                    public_key,
                    requested_at: Instant::now(),
                });
            }
            TuiEvent::ApprovalHandled { device_id, .. } => {
                self.approvals.retain(|a| a.device_id != device_id);
            }
            TuiEvent::StatsUpdate {
                devices_connected,
                sessions_active,
                uptime_secs,
            } => {
                self.stats.devices_connected = devices_connected;
                self.stats.sessions_active = sessions_active;
                self.stats.uptime_secs = uptime_secs;
            }
            TuiEvent::Refresh => {
                // Just trigger a redraw
            }
            TuiEvent::Shutdown => {
                self.should_quit = true;
            }
        }
    }

    /// Returns whether the TUI should quit.
    pub fn should_quit(&self) -> bool {
        self.should_quit
    }

    /// Requests the TUI to quit.
    pub fn quit(&mut self) {
        self.should_quit = true;
    }

    /// Draws the TUI.
    pub fn draw(&mut self) -> io::Result<()> {
        // Update uptime from start time
        self.stats.uptime_secs = self.start_time.elapsed().as_secs();

        // Clone state for drawing
        let current_tab = self.current_tab;
        let stats = self.stats.clone();
        let devices = self.devices.clone();
        let sessions = self.sessions.clone();
        let approvals = self.approvals.clone();
        let always_trust = self.always_trust;
        let mut list_state = self.list_state.clone();

        self.terminal.draw(|frame| {
            Self::render_frame(
                frame,
                current_tab,
                &stats,
                &devices,
                &sessions,
                &approvals,
                always_trust,
                &mut list_state,
            );
        })?;

        self.list_state = list_state;
        Ok(())
    }

    /// Renders the entire frame.
    #[allow(clippy::too_many_arguments)]
    fn render_frame(
        frame: &mut Frame,
        current_tab: Tab,
        stats: &DaemonStats,
        devices: &[DeviceInfo],
        sessions: &[SessionInfo],
        approvals: &[ApprovalInfo],
        always_trust: bool,
        list_state: &mut ListState,
    ) {
        let size = frame.area();

        // Create main layout: tabs at top, content in middle, status at bottom
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Tab bar
                Constraint::Min(0),    // Content
                Constraint::Length(3), // Status bar
            ])
            .split(size);

        // Render tab bar
        Self::render_tabs(frame, chunks[0], current_tab);

        // Render content based on current tab
        match current_tab {
            Tab::Status => Self::render_status_tab(frame, chunks[1], stats, devices, sessions),
            Tab::Sessions => Self::render_sessions_tab(frame, chunks[1], sessions, list_state),
            Tab::Devices => Self::render_devices_tab(frame, chunks[1], devices, list_state),
            Tab::Approvals => {
                Self::render_approvals_tab(frame, chunks[1], approvals, always_trust, list_state)
            }
        }

        // Render status bar
        Self::render_status_bar(frame, chunks[2], stats);
    }

    /// Renders the tab bar.
    fn render_tabs(frame: &mut Frame, area: Rect, current_tab: Tab) {
        let titles: Vec<Line> = Tab::all()
            .iter()
            .map(|t| {
                let style = if *t == current_tab {
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                Line::from(vec![Span::styled(format!(" {} ", t.title()), style)])
            })
            .collect();

        let tabs = Tabs::new(titles)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" RemoShell Daemon "),
            )
            .select(current_tab.index())
            .style(Style::default().fg(Color::White))
            .highlight_style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            );

        frame.render_widget(tabs, area);
    }

    /// Renders the status tab content.
    fn render_status_tab(
        frame: &mut Frame,
        area: Rect,
        stats: &DaemonStats,
        devices: &[DeviceInfo],
        sessions: &[SessionInfo],
    ) {
        let connected_devices = devices.iter().filter(|d| d.connected).count();

        let uptime = format_duration(stats.uptime_secs);

        let text = vec![
            Line::from(vec![
                Span::styled("Daemon Status: ", Style::default().fg(Color::Gray)),
                Span::styled(
                    "Running",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("Uptime: ", Style::default().fg(Color::Gray)),
                Span::styled(uptime, Style::default().fg(Color::Cyan)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("Connected Devices: ", Style::default().fg(Color::Gray)),
                Span::styled(
                    connected_devices.to_string(),
                    Style::default().fg(Color::Yellow),
                ),
            ]),
            Line::from(vec![
                Span::styled("Active Sessions: ", Style::default().fg(Color::Gray)),
                Span::styled(
                    sessions.len().to_string(),
                    Style::default().fg(Color::Yellow),
                ),
            ]),
            Line::from(""),
            Line::from(vec![Span::styled(
                "Press Tab/Arrow keys to switch tabs, q to quit",
                Style::default().fg(Color::DarkGray),
            )]),
        ];

        let paragraph = Paragraph::new(text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Status Overview "),
            )
            .style(Style::default().fg(Color::White));

        frame.render_widget(paragraph, area);
    }

    /// Renders the sessions tab content.
    fn render_sessions_tab(
        frame: &mut Frame,
        area: Rect,
        sessions: &[SessionInfo],
        list_state: &mut ListState,
    ) {
        let items: Vec<ListItem> = sessions
            .iter()
            .map(|s| {
                let content = Line::from(vec![
                    Span::styled(&s.id[..8.min(s.id.len())], Style::default().fg(Color::Cyan)),
                    Span::raw(" | Device: "),
                    Span::styled(
                        &s.device_id[..8.min(s.device_id.len())],
                        Style::default().fg(Color::Yellow),
                    ),
                    Span::raw(format!(" | {}x{}", s.size.0, s.size.1)),
                ]);
                ListItem::new(content)
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!(" Sessions ({}) ", sessions.len())),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("> ");

        frame.render_stateful_widget(list, area, list_state);
    }

    /// Renders the devices tab content.
    fn render_devices_tab(
        frame: &mut Frame,
        area: Rect,
        devices: &[DeviceInfo],
        list_state: &mut ListState,
    ) {
        let items: Vec<ListItem> = devices
            .iter()
            .map(|d| {
                let status = if d.connected {
                    Span::styled("Online", Style::default().fg(Color::Green))
                } else {
                    Span::styled("Offline", Style::default().fg(Color::Red))
                };
                let content = Line::from(vec![
                    Span::styled(&d.name, Style::default().fg(Color::Cyan)),
                    Span::raw(" | "),
                    Span::styled(
                        &d.id[..8.min(d.id.len())],
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::raw(" | "),
                    status,
                ]);
                ListItem::new(content)
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!(" Devices ({}) ", devices.len())),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("> ");

        frame.render_stateful_widget(list, area, list_state);
    }

    /// Renders the approvals tab content.
    fn render_approvals_tab(
        frame: &mut Frame,
        area: Rect,
        approvals: &[ApprovalInfo],
        always_trust: bool,
        list_state: &mut ListState,
    ) {
        // Split the area: list at top, help text at bottom
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(5),    // Approval list
                Constraint::Length(4), // Help text and status
            ])
            .split(area);

        // Render the approvals list
        let items: Vec<ListItem> = approvals
            .iter()
            .map(|a| {
                let elapsed = a.requested_at.elapsed();
                let age = format_duration(elapsed.as_secs());

                // Format IP address
                let ip_str = match &a.ip_address {
                    Some(ip) => ip.to_string(),
                    None => "unknown".to_string(),
                };

                // Truncate device ID for display
                let truncated_id = if a.device_id.len() > 12 {
                    format!("{}...", &a.device_id[..12])
                } else {
                    a.device_id.clone()
                };

                let content = Line::from(vec![
                    Span::styled(
                        &a.name,
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(" | ID: "),
                    Span::styled(truncated_id, Style::default().fg(Color::DarkGray)),
                    Span::raw(" | IP: "),
                    Span::styled(ip_str, Style::default().fg(Color::Cyan)),
                    Span::raw(" | Waiting: "),
                    Span::styled(age, Style::default().fg(Color::Magenta)),
                ]);
                ListItem::new(content)
            })
            .collect();

        let title = if approvals.is_empty() {
            " Pending Approvals (none) ".to_string()
        } else {
            format!(" Pending Approvals ({}) ", approvals.len())
        };

        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title(title))
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("> ");

        frame.render_stateful_widget(list, chunks[0], list_state);

        // Render help text with always_trust checkbox
        let trust_checkbox = if always_trust {
            Span::styled(
                "[X] Always trust",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            Span::styled("[ ] Always trust", Style::default().fg(Color::Gray))
        };

        let help_lines = vec![
            Line::from(vec![
                Span::styled(
                    "  A",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(": Accept  "),
                Span::styled(
                    "R",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ),
                Span::raw(": Reject  "),
                Span::styled(
                    "T",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(": Toggle trust  "),
                trust_checkbox,
            ]),
            Line::from(vec![
                Span::styled("  j/k", Style::default().fg(Color::Cyan)),
                Span::raw(" or "),
                Span::styled("Up/Down", Style::default().fg(Color::Cyan)),
                Span::raw(": Navigate  "),
                Span::styled("Enter", Style::default().fg(Color::Cyan)),
                Span::raw(": Accept"),
            ]),
        ];

        let help_paragraph = Paragraph::new(help_lines)
            .block(Block::default().borders(Borders::ALL).title(" Actions "))
            .style(Style::default().fg(Color::White));

        frame.render_widget(help_paragraph, chunks[1]);
    }

    /// Renders the status bar at the bottom.
    fn render_status_bar(frame: &mut Frame, area: Rect, stats: &DaemonStats) {
        let uptime = format_duration(stats.uptime_secs);

        let status_text = Line::from(vec![
            Span::styled(" Devices: ", Style::default().fg(Color::Gray)),
            Span::styled(
                stats.devices_connected.to_string(),
                Style::default().fg(Color::Cyan),
            ),
            Span::styled(" | Sessions: ", Style::default().fg(Color::Gray)),
            Span::styled(
                stats.sessions_active.to_string(),
                Style::default().fg(Color::Cyan),
            ),
            Span::styled(" | Uptime: ", Style::default().fg(Color::Gray)),
            Span::styled(uptime, Style::default().fg(Color::Cyan)),
            Span::styled(
                " | Press 'q' to quit ",
                Style::default().fg(Color::DarkGray),
            ),
        ]);

        let paragraph = Paragraph::new(status_text)
            .block(Block::default().borders(Borders::ALL))
            .style(Style::default().fg(Color::White));

        frame.render_widget(paragraph, area);
    }

    /// Runs the main event loop.
    pub async fn run(&mut self) -> io::Result<()> {
        let tick_rate = Duration::from_millis(250);
        let mut last_tick = Instant::now();

        loop {
            // Draw the UI
            self.draw()?;

            // Calculate timeout for polling
            let timeout = tick_rate
                .checked_sub(last_tick.elapsed())
                .unwrap_or_else(|| Duration::from_secs(0));

            // Poll for crossterm events with timeout
            if event::poll(timeout)? {
                if let Event::Key(key) = event::read()? {
                    self.handle_key(key);
                }
            }

            // Process any pending TUI events from daemon
            while let Ok(event) = self.event_rx.try_recv() {
                self.handle_event(event);
            }

            // Check for quit
            if self.should_quit {
                break;
            }

            // Update tick
            if last_tick.elapsed() >= tick_rate {
                last_tick = Instant::now();
            }
        }

        Ok(())
    }

    /// Restores the terminal to its original state.
    pub fn restore(&mut self) -> io::Result<()> {
        disable_raw_mode()?;
        execute!(
            self.terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        self.terminal.show_cursor()?;
        Ok(())
    }
}

impl Drop for TuiApp {
    fn drop(&mut self) {
        // Best effort cleanup
        let _ = disable_raw_mode();
        let _ = execute!(
            self.terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        );
        let _ = self.terminal.show_cursor();
    }
}

/// Formats a duration in seconds to a human-readable string.
fn format_duration(secs: u64) -> String {
    let hours = secs / 3600;
    let minutes = (secs % 3600) / 60;
    let seconds = secs % 60;

    if hours > 0 {
        format!("{}h {}m {}s", hours, minutes, seconds)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, seconds)
    } else {
        format!("{}s", seconds)
    }
}

/// Parses a device ID from a hex string.
///
/// The hex string should be 32 characters (16 bytes in hex).
fn parse_device_id_from_hex(hex: &str) -> anyhow::Result<protocol::DeviceId> {
    let bytes = hex::decode(hex).map_err(|e| anyhow::anyhow!("Invalid hex string: {}", e))?;

    if bytes.len() != 16 {
        return Err(anyhow::anyhow!(
            "Invalid device ID length: expected 16 bytes, got {}",
            bytes.len()
        ));
    }

    let mut id_bytes = [0u8; 16];
    id_bytes.copy_from_slice(&bytes);
    Ok(protocol::DeviceId::from_bytes(id_bytes))
}

/// Converts a device ID to a hex string.
pub fn device_id_to_hex(device_id: &protocol::DeviceId) -> String {
    hex::encode(device_id.as_bytes())
}

/// Processes an approval result and updates the trust store if needed.
///
/// This function should be called by the daemon when it receives an approval
/// result from the TUI. It handles:
/// - Adding devices to the trust store when accepted with "always trust"
/// - Setting the trust level to Revoked when rejected with "always trust"
///
/// # Arguments
/// * `result` - The approval result from the TUI
/// * `trust_store` - The trust store to update
///
/// # Returns
/// * `Ok(())` if the operation succeeded
/// * `Err(...)` if there was an error updating the trust store
///
/// # Example
/// ```ignore
/// use daemon::ui::tui::{process_approval_result, ApprovalResult, ApprovalAction};
/// use daemon::devices::TrustStore;
///
/// let trust_store = TrustStore::with_default_path();
/// trust_store.load().unwrap();
///
/// // Handle approval results from the TUI
/// while let Some(result) = approval_rx.recv().await {
///     if let Err(e) = process_approval_result(&result, &trust_store) {
///         tracing::error!("Failed to process approval: {}", e);
///     }
/// }
/// ```
pub fn process_approval_result(
    result: &ApprovalResult,
    trust_store: &crate::devices::TrustStore,
) -> anyhow::Result<()> {
    use crate::devices::{TrustLevel, TrustedDevice};

    // Only update trust store if always_trust is enabled
    if !result.always_trust {
        tracing::debug!(
            "Approval for {} handled without updating trust store (always_trust=false)",
            result.device_id
        );
        return Ok(());
    }

    // Need public key for trust store operations
    let public_key = match result.public_key {
        Some(pk) => pk,
        None => {
            tracing::warn!(
                "Cannot add device {} to trust store: no public key available",
                result.device_id
            );
            return Ok(());
        }
    };

    // Parse device ID from hex string
    let device_id = parse_device_id_from_hex(&result.device_id)?;

    match result.action {
        ApprovalAction::Accept => {
            // Create a trusted device entry
            let trusted_device =
                TrustedDevice::new(device_id, result.device_name.clone(), public_key);

            tracing::info!(
                "Adding device {} ({}) to trust store",
                result.device_id,
                result.device_name
            );

            trust_store.add_device(trusted_device)?;
            trust_store.save()?;
        }
        ApprovalAction::Reject => {
            // Create a revoked device entry
            let mut revoked_device =
                TrustedDevice::new(device_id, result.device_name.clone(), public_key);
            revoked_device.trust_level = TrustLevel::Revoked;

            tracing::info!(
                "Adding device {} ({}) to trust store as revoked",
                result.device_id,
                result.device_name
            );

            trust_store.add_device(revoked_device)?;
            trust_store.save()?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tab_index() {
        assert_eq!(Tab::Status.index(), 0);
        assert_eq!(Tab::Sessions.index(), 1);
        assert_eq!(Tab::Devices.index(), 2);
        assert_eq!(Tab::Approvals.index(), 3);
    }

    #[test]
    fn test_tab_from_index() {
        assert_eq!(Tab::from_index(0), Tab::Status);
        assert_eq!(Tab::from_index(1), Tab::Sessions);
        assert_eq!(Tab::from_index(2), Tab::Devices);
        assert_eq!(Tab::from_index(3), Tab::Approvals);
        assert_eq!(Tab::from_index(99), Tab::Status); // Out of bounds defaults to Status
    }

    #[test]
    fn test_tab_next() {
        assert_eq!(Tab::Status.next(), Tab::Sessions);
        assert_eq!(Tab::Sessions.next(), Tab::Devices);
        assert_eq!(Tab::Devices.next(), Tab::Approvals);
        assert_eq!(Tab::Approvals.next(), Tab::Status); // Wraps around
    }

    #[test]
    fn test_tab_prev() {
        assert_eq!(Tab::Status.prev(), Tab::Approvals); // Wraps around
        assert_eq!(Tab::Sessions.prev(), Tab::Status);
        assert_eq!(Tab::Devices.prev(), Tab::Sessions);
        assert_eq!(Tab::Approvals.prev(), Tab::Devices);
    }

    #[test]
    fn test_tab_all() {
        let all = Tab::all();
        assert_eq!(all.len(), 4);
        assert_eq!(all[0], Tab::Status);
        assert_eq!(all[1], Tab::Sessions);
        assert_eq!(all[2], Tab::Devices);
        assert_eq!(all[3], Tab::Approvals);
    }

    #[test]
    fn test_tab_title() {
        assert_eq!(Tab::Status.title(), "Status");
        assert_eq!(Tab::Sessions.title(), "Sessions");
        assert_eq!(Tab::Devices.title(), "Devices");
        assert_eq!(Tab::Approvals.title(), "Approvals");
    }

    #[test]
    fn test_format_duration_seconds() {
        assert_eq!(format_duration(0), "0s");
        assert_eq!(format_duration(30), "30s");
        assert_eq!(format_duration(59), "59s");
    }

    #[test]
    fn test_format_duration_minutes() {
        assert_eq!(format_duration(60), "1m 0s");
        assert_eq!(format_duration(90), "1m 30s");
        assert_eq!(format_duration(3599), "59m 59s");
    }

    #[test]
    fn test_format_duration_hours() {
        assert_eq!(format_duration(3600), "1h 0m 0s");
        assert_eq!(format_duration(3661), "1h 1m 1s");
        assert_eq!(format_duration(86400), "24h 0m 0s");
    }

    #[test]
    fn test_default_tab() {
        let tab: Tab = Default::default();
        assert_eq!(tab, Tab::Status);
    }

    #[test]
    fn test_daemon_stats_default() {
        let stats = DaemonStats::default();
        assert_eq!(stats.devices_connected, 0);
        assert_eq!(stats.sessions_active, 0);
        assert_eq!(stats.uptime_secs, 0);
    }

    #[test]
    fn test_tui_event_variants() {
        // Test that all event variants can be created
        let _ = TuiEvent::DeviceConnected {
            device_id: "test".to_string(),
            name: "Test Device".to_string(),
        };
        let _ = TuiEvent::DeviceDisconnected {
            device_id: "test".to_string(),
        };
        let _ = TuiEvent::SessionCreated {
            session_id: "sess1".to_string(),
            device_id: "dev1".to_string(),
        };
        let _ = TuiEvent::SessionTerminated {
            session_id: "sess1".to_string(),
        };
        let _ = TuiEvent::ApprovalRequested {
            device_id: "dev1".to_string(),
            name: "Device".to_string(),
            ip_address: None,
            public_key: None,
        };
        let _ = TuiEvent::ApprovalHandled {
            device_id: "dev1".to_string(),
            approved: true,
        };
        let _ = TuiEvent::StatsUpdate {
            devices_connected: 5,
            sessions_active: 3,
            uptime_secs: 1000,
        };
        let _ = TuiEvent::Refresh;
        let _ = TuiEvent::Shutdown;
    }

    #[test]
    fn test_device_info() {
        let device = DeviceInfo {
            id: "device-123".to_string(),
            name: "My Device".to_string(),
            connected: true,
        };
        assert_eq!(device.id, "device-123");
        assert_eq!(device.name, "My Device");
        assert!(device.connected);
    }

    #[test]
    fn test_session_info() {
        let session = SessionInfo {
            id: "session-456".to_string(),
            device_id: "device-123".to_string(),
            size: (80, 24),
        };
        assert_eq!(session.id, "session-456");
        assert_eq!(session.device_id, "device-123");
        assert_eq!(session.size, (80, 24));
    }

    #[test]
    fn test_approval_info() {
        use std::net::Ipv4Addr;

        let approval = ApprovalInfo {
            device_id: "device-789".to_string(),
            name: "New Device".to_string(),
            ip_address: Some(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100))),
            public_key: Some([42u8; 32]),
            requested_at: Instant::now(),
        };
        assert_eq!(approval.device_id, "device-789");
        assert_eq!(approval.name, "New Device");
        assert_eq!(
            approval.ip_address,
            Some(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)))
        );
        assert_eq!(approval.public_key, Some([42u8; 32]));
        assert!(approval.requested_at.elapsed().as_secs() < 1);
    }

    #[test]
    fn test_approval_info_without_optional_fields() {
        let approval = ApprovalInfo {
            device_id: "device-789".to_string(),
            name: "New Device".to_string(),
            ip_address: None,
            public_key: None,
            requested_at: Instant::now(),
        };
        assert_eq!(approval.device_id, "device-789");
        assert_eq!(approval.name, "New Device");
        assert!(approval.ip_address.is_none());
        assert!(approval.public_key.is_none());
    }

    #[test]
    fn test_approval_action_variants() {
        assert_eq!(ApprovalAction::Accept, ApprovalAction::Accept);
        assert_eq!(ApprovalAction::Reject, ApprovalAction::Reject);
        assert_ne!(ApprovalAction::Accept, ApprovalAction::Reject);
    }

    #[test]
    fn test_approval_result_accept() {
        let result = ApprovalResult {
            device_id: "dev-123".to_string(),
            action: ApprovalAction::Accept,
            always_trust: true,
            device_name: "Test Device".to_string(),
            public_key: Some([1u8; 32]),
        };
        assert_eq!(result.device_id, "dev-123");
        assert_eq!(result.action, ApprovalAction::Accept);
        assert!(result.always_trust);
        assert_eq!(result.device_name, "Test Device");
        assert!(result.public_key.is_some());
    }

    #[test]
    fn test_approval_result_reject() {
        let result = ApprovalResult {
            device_id: "dev-456".to_string(),
            action: ApprovalAction::Reject,
            always_trust: false,
            device_name: "Bad Device".to_string(),
            public_key: None,
        };
        assert_eq!(result.device_id, "dev-456");
        assert_eq!(result.action, ApprovalAction::Reject);
        assert!(!result.always_trust);
        assert_eq!(result.device_name, "Bad Device");
        assert!(result.public_key.is_none());
    }

    // Tests that require a terminal cannot run in CI without a TTY
    // The following tests verify logic without needing actual terminal access

    #[tokio::test]
    async fn test_event_handling_device_connected() {
        let (tx, mut rx) = mpsc::channel::<TuiEvent>(10);

        // Simulate sending an event
        tx.send(TuiEvent::DeviceConnected {
            device_id: "dev1".to_string(),
            name: "Test Device".to_string(),
        })
        .await
        .unwrap();

        // Verify it can be received
        let event = rx.recv().await.unwrap();
        match event {
            TuiEvent::DeviceConnected { device_id, name } => {
                assert_eq!(device_id, "dev1");
                assert_eq!(name, "Test Device");
            }
            _ => panic!("Unexpected event type"),
        }
    }

    #[tokio::test]
    async fn test_event_handling_session_lifecycle() {
        let (tx, mut rx) = mpsc::channel::<TuiEvent>(10);

        // Create session
        tx.send(TuiEvent::SessionCreated {
            session_id: "sess1".to_string(),
            device_id: "dev1".to_string(),
        })
        .await
        .unwrap();

        // Terminate session
        tx.send(TuiEvent::SessionTerminated {
            session_id: "sess1".to_string(),
        })
        .await
        .unwrap();

        // Verify events
        let event1 = rx.recv().await.unwrap();
        assert!(matches!(event1, TuiEvent::SessionCreated { .. }));

        let event2 = rx.recv().await.unwrap();
        assert!(matches!(event2, TuiEvent::SessionTerminated { .. }));
    }

    #[tokio::test]
    async fn test_stats_update() {
        let (tx, mut rx) = mpsc::channel::<TuiEvent>(10);

        tx.send(TuiEvent::StatsUpdate {
            devices_connected: 10,
            sessions_active: 5,
            uptime_secs: 3600,
        })
        .await
        .unwrap();

        let event = rx.recv().await.unwrap();
        match event {
            TuiEvent::StatsUpdate {
                devices_connected,
                sessions_active,
                uptime_secs,
            } => {
                assert_eq!(devices_connected, 10);
                assert_eq!(sessions_active, 5);
                assert_eq!(uptime_secs, 3600);
            }
            _ => panic!("Unexpected event type"),
        }
    }

    #[tokio::test]
    async fn test_approval_requested_event_with_ip() {
        use std::net::Ipv4Addr;

        let (tx, mut rx) = mpsc::channel::<TuiEvent>(10);

        tx.send(TuiEvent::ApprovalRequested {
            device_id: "dev-approval".to_string(),
            name: "Approval Device".to_string(),
            ip_address: Some(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))),
            public_key: Some([99u8; 32]),
        })
        .await
        .unwrap();

        let event = rx.recv().await.unwrap();
        match event {
            TuiEvent::ApprovalRequested {
                device_id,
                name,
                ip_address,
                public_key,
            } => {
                assert_eq!(device_id, "dev-approval");
                assert_eq!(name, "Approval Device");
                assert_eq!(ip_address, Some(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))));
                assert_eq!(public_key, Some([99u8; 32]));
            }
            _ => panic!("Unexpected event type"),
        }
    }

    #[tokio::test]
    async fn test_approval_result_channel() {
        let (tx, mut rx) = mpsc::channel::<ApprovalResult>(10);

        let result = ApprovalResult {
            device_id: "test-device".to_string(),
            action: ApprovalAction::Accept,
            always_trust: true,
            device_name: "Test".to_string(),
            public_key: Some([0u8; 32]),
        };

        tx.send(result.clone()).await.unwrap();

        let received = rx.recv().await.unwrap();
        assert_eq!(received.device_id, "test-device");
        assert_eq!(received.action, ApprovalAction::Accept);
        assert!(received.always_trust);
    }

    #[test]
    fn test_approval_action_is_copy() {
        // Verify ApprovalAction is Copy
        let action = ApprovalAction::Accept;
        let action_copy = action;
        assert_eq!(action, action_copy);
    }

    #[test]
    fn test_approval_result_clone() {
        let result = ApprovalResult {
            device_id: "clone-test".to_string(),
            action: ApprovalAction::Reject,
            always_trust: false,
            device_name: "Clone Device".to_string(),
            public_key: None,
        };

        let cloned = result.clone();
        assert_eq!(result.device_id, cloned.device_id);
        assert_eq!(result.action, cloned.action);
        assert_eq!(result.always_trust, cloned.always_trust);
        assert_eq!(result.device_name, cloned.device_name);
        assert_eq!(result.public_key, cloned.public_key);
    }

    #[test]
    fn test_process_approval_result_without_always_trust() {
        use crate::devices::TrustStore;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("test_trust_store.json");
        let trust_store = TrustStore::new(&path);

        let result = ApprovalResult {
            device_id: "test-device-id-12345678".to_string(),
            action: ApprovalAction::Accept,
            always_trust: false, // Not saving to trust store
            device_name: "Test Device".to_string(),
            public_key: Some([1u8; 32]),
        };

        // Should succeed without modifying trust store
        let res = process_approval_result(&result, &trust_store);
        assert!(res.is_ok());

        // Trust store should be empty
        assert!(trust_store.is_empty().unwrap());
    }

    #[test]
    fn test_process_approval_result_without_public_key() {
        use crate::devices::TrustStore;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("test_trust_store.json");
        let trust_store = TrustStore::new(&path);

        let result = ApprovalResult {
            device_id: "test-device-id-12345678".to_string(),
            action: ApprovalAction::Accept,
            always_trust: true, // Want to save, but no public key
            device_name: "Test Device".to_string(),
            public_key: None, // No public key available
        };

        // Should succeed but not modify trust store (warning logged)
        let res = process_approval_result(&result, &trust_store);
        assert!(res.is_ok());

        // Trust store should be empty (no public key to store)
        assert!(trust_store.is_empty().unwrap());
    }

    #[test]
    fn test_process_approval_result_accept_with_always_trust() {
        use crate::devices::TrustStore;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("test_trust_store.json");
        let trust_store = TrustStore::new(&path);

        // Generate a valid device identity for testing
        let identity = protocol::DeviceIdentity::generate();
        let device_id_hex = device_id_to_hex(identity.device_id());

        let result = ApprovalResult {
            device_id: device_id_hex.clone(),
            action: ApprovalAction::Accept,
            always_trust: true,
            device_name: "Trusted Device".to_string(),
            public_key: Some(identity.public_key_bytes()),
        };

        // Process the approval
        let res = process_approval_result(&result, &trust_store);
        assert!(res.is_ok());

        // Verify device was added to trust store
        assert_eq!(trust_store.len().unwrap(), 1);
        assert!(trust_store.is_trusted(identity.device_id()).unwrap());
    }

    #[test]
    fn test_process_approval_result_reject_with_always_trust() {
        use crate::devices::{TrustLevel, TrustStore};
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("test_trust_store.json");
        let trust_store = TrustStore::new(&path);

        // Generate a valid device identity for testing
        let identity = protocol::DeviceIdentity::generate();
        let device_id_hex = device_id_to_hex(identity.device_id());

        let result = ApprovalResult {
            device_id: device_id_hex.clone(),
            action: ApprovalAction::Reject,
            always_trust: true, // Permanently reject
            device_name: "Blocked Device".to_string(),
            public_key: Some(identity.public_key_bytes()),
        };

        // Process the rejection
        let res = process_approval_result(&result, &trust_store);
        assert!(res.is_ok());

        // Verify device was added to trust store as revoked
        assert_eq!(trust_store.len().unwrap(), 1);
        assert!(!trust_store.is_trusted(identity.device_id()).unwrap());

        // Verify it's specifically marked as revoked
        let device = trust_store
            .get_device(identity.device_id())
            .unwrap()
            .unwrap();
        assert_eq!(device.trust_level, TrustLevel::Revoked);
    }

    #[test]
    fn test_process_approval_result_invalid_device_id() {
        use crate::devices::TrustStore;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("test_trust_store.json");
        let trust_store = TrustStore::new(&path);

        let result = ApprovalResult {
            device_id: "not-a-valid-hex-device-id".to_string(),
            action: ApprovalAction::Accept,
            always_trust: true,
            device_name: "Invalid Device".to_string(),
            public_key: Some([1u8; 32]),
        };

        // Should fail due to invalid device ID
        let res = process_approval_result(&result, &trust_store);
        assert!(res.is_err());
    }
}
