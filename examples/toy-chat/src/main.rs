mod protocol;

use crate::protocol::{Chat2Message, TOY_CHAT_CONTENT_TOPIC};
use chrono::Utc;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use prost::Message;
use std::io::Write;
use std::os::unix::io::IntoRawFd;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use std::{error::Error, io};
use tokio::task;
use tui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Span, Spans, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame, Terminal,
};
use unicode_width::UnicodeWidthStr;
use waku::{
    general::pubsubtopic::PubsubTopic, general::Result, waku_new, Initialized, LibwakuResponse,
    Running, WakuEvent, WakuMessage, WakuNodeConfig, WakuNodeHandle,
};

enum InputMode {
    Normal,
    Editing,
}

const STORE_NODE: &str = "/dns4/store-01.do-ams3.status.staging.status.im/tcp/30303/p2p/16Uiu2HAm3xVDaz6SRJ6kErwC21zBJEZjavVXg7VSkoWzaV1aMA3F";

const DEFAULT_PUBSUB_TOPIC: &str = "/waku/2/rs/16/32";

/// App holds the state of the application
struct App<State> {
    /// Current value of the input box
    input: String,
    nick: String,
    /// Current input mode
    input_mode: InputMode,
    /// History of recorded messages
    messages: Arc<RwLock<Vec<Chat2Message>>>,
    waku: WakuNodeHandle<State>,
}

impl App<Initialized> {
    async fn new(nick: String) -> Result<App<Initialized>> {
        let pubsub_topic = PubsubTopic::new(DEFAULT_PUBSUB_TOPIC);
        let waku = waku_new(Some(WakuNodeConfig {
            tcp_port: Some(60010),
            cluster_id: Some(16),
            shards: vec![1, 32, 64, 128, 256],
            num_shards_in_network: Some(1024),
            // node_key: Some(SecretKey::from_str("2fc0515879e52b7b73297cfd6ab3abf7c344ef84b7a90ff6f4cc19e05a198027").unwrap()),
            max_message_size: Some("1024KiB".to_string()),
            relay_topics: vec![String::from(&pubsub_topic)],
            log_level: Some("FATAL"), // Supported: TRACE, DEBUG, INFO, NOTICE, WARN, ERROR or FATAL
            keep_alive: Some(true),
            // Discovery
            dns_discovery: Some(true),
            dns_discovery_url: Some("enrtree://AMOJVZX4V6EXP7NTJPMAYJYST2QP6AJXYW76IU6VGJS7UVSNDYZG4@boot.prod.status.nodes.status.im"),
            // discv5_discovery: Some(true),
            // discv5_udp_port: Some(9001),
            // discv5_enr_auto_update: Some(false),
            ..Default::default()
        })).await?;

        Ok(App {
            input: String::new(),
            input_mode: InputMode::Normal,
            messages: Arc::new(RwLock::new(Vec::new())),
            nick,
            waku,
        })
    }

    async fn start_waku_node(self) -> Result<App<Running>> {
        let shared_messages = Arc::clone(&self.messages);

        self.waku.set_event_callback(move |response| {
            if let LibwakuResponse::Success(v) = response {
                let event: WakuEvent = serde_json::from_str(v.unwrap().as_str())
                    .expect("failed parsing event in set_event_callback");

                match event {
                    WakuEvent::WakuMessage(evt) => {
                        if evt.waku_message.content_topic != TOY_CHAT_CONTENT_TOPIC {
                            return; // skip the messages that don't belong to the toy chat
                        }

                        match <Chat2Message as Message>::decode(evt.waku_message.payload()) {
                            Ok(chat_message) => {
                                // Add the new message to the front
                                {
                                    let mut messages_lock = shared_messages.write().unwrap();
                                    messages_lock.insert(0, chat_message); // Insert at the front (index 0)
                                }
                            }
                            Err(e) => {
                                let mut out = std::io::stderr();
                                write!(out, "{e:?}").unwrap();
                            }
                        }
                    }
                    WakuEvent::RelayTopicHealthChange(_evt) => {
                        // dbg!("Relay topic change evt", evt);
                    }
                    WakuEvent::ConnectionChange(_evt) => {
                        // dbg!("Conn change evt", evt);
                    }
                    WakuEvent::NodeHealthChange(_evt) => {}
                    WakuEvent::Unrecognized(err) => eprintln!("Unrecognized waku event: {:?}", err),
                    _ => eprintln!("event case not expected"),
                };
            }
        })?;

        let waku = self.waku.start().await?;

        Ok(App {
            input: self.input,
            nick: self.nick,
            input_mode: self.input_mode,
            messages: self.messages,
            waku,
        })
    }
}

impl App<Running> {
    async fn retrieve_history(&mut self) {
        let one_day_in_secs = 60 * 60 * 24;
        let time_start = (Duration::from_secs(Utc::now().timestamp() as u64)
            - Duration::from_secs(one_day_in_secs))
        .as_nanos() as u64;

        let include_data = true;

        let messages = self
            .waku
            .store_query(
                None,
                vec![TOY_CHAT_CONTENT_TOPIC.clone()],
                STORE_NODE,
                include_data,
                Some(time_start),
                None,
                None,
            )
            .await
            .unwrap();

        let messages: Vec<_> = messages
            .into_iter()
            // we expect messages because the query was passed with include_data == true
            .filter(|item| item.message.is_some())
            .map(|store_resp_msg| {
                <Chat2Message as Message>::decode(store_resp_msg.message.unwrap().payload())
                    .expect("Toy chat messages should be decodeable")
            })
            .collect();

        if !messages.is_empty() {
            *self.messages.write().unwrap() = messages;
        }
    }

    fn run_main_loop<B: Backend>(
        &mut self,
        terminal: &mut Terminal<B>,
    ) -> std::result::Result<(), Box<dyn Error>> {
        loop {
            terminal.draw(|f| ui(f, self))?;

            if event::poll(Duration::from_millis(500)).unwrap() {
                if let Event::Key(key) = event::read()? {
                    match self.input_mode {
                        InputMode::Normal => match key.code {
                            KeyCode::Char('e') => {
                                self.input_mode = InputMode::Editing;
                            }
                            KeyCode::Char('q') => {
                                return Ok(());
                            }
                            _ => {}
                        },
                        InputMode::Editing => match key.code {
                            KeyCode::Enter => {
                                let message_content: String = self.input.drain(..).collect();
                                let message = Chat2Message::new(&self.nick, &message_content);
                                let mut buff = Vec::new();
                                let meta = Vec::new();
                                Message::encode(&message, &mut buff)?;
                                let waku_message = WakuMessage::new(
                                    buff,
                                    TOY_CHAT_CONTENT_TOPIC.clone(),
                                    1,
                                    meta,
                                    false,
                                );

                                // Call the async function in a blocking context
                                task::block_in_place(|| {
                                    // Obtain the current runtime handle
                                    let handle = tokio::runtime::Handle::current();

                                    // Block on the async function
                                    handle.block_on(async {
                                        // Assuming `self` is available in the current context
                                        let pubsub_topic = PubsubTopic::new(DEFAULT_PUBSUB_TOPIC);
                                        if let Err(e) = self
                                            .waku
                                            .relay_publish_message(
                                                &waku_message,
                                                &pubsub_topic,
                                                None,
                                            )
                                            .await
                                        {
                                            let mut out = std::io::stderr();
                                            write!(out, "{e:?}").unwrap();
                                        }
                                    });
                                });
                            }
                            KeyCode::Char(c) => {
                                self.input.push(c);
                            }
                            KeyCode::Backspace => {
                                self.input.pop();
                            }
                            KeyCode::Esc => {
                                self.input_mode = InputMode::Normal;
                            }
                            _ => {}
                        },
                    }
                }
            }
        }
    }

    async fn stop_app(self) {
        self.waku
            .stop()
            .await
            .expect("the node should stop properly");
    }
}

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn Error>> {
    let nick = std::env::args().nth(1).expect("Nick to be set");

    let app = App::new(nick).await?;
    let mut app = app.start_waku_node().await?;

    // Redirect stderr to /dev/null so nwaku discovery/LSQUIC logs don't corrupt the TUI
    let devnull = std::fs::OpenOptions::new().write(true).open("/dev/null")?;
    unsafe {
        libc::dup2(devnull.into_raw_fd(), 2);
    }

    // setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Fetch history in the background so the TUI starts immediately
    tokio::spawn(async move {
        // small delay so the node has time to connect to peers before querying
        tokio::time::sleep(Duration::from_secs(3)).await;
    });

    let res = app.run_main_loop(&mut terminal);
    app.stop_app().await;

    // restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{err:?}")
    }
    Ok(())
}

fn ui<B: Backend, State>(f: &mut Frame<B>, app: &App<State>) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints(
            [
                Constraint::Length(1),
                Constraint::Length(3),
                Constraint::Min(1),
            ]
            .as_ref(),
        )
        .split(f.size());

    let (msg, style) = match app.input_mode {
        InputMode::Normal => (
            vec![
                Span::raw("Press "),
                Span::styled("q", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" to exit, "),
                Span::styled("e", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" to start writing a message."),
            ],
            Style::default().add_modifier(Modifier::RAPID_BLINK),
        ),
        InputMode::Editing => (
            vec![
                Span::raw("Press "),
                Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" to stop editing, "),
                Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" to record the message"),
            ],
            Style::default(),
        ),
    };
    let mut text = Text::from(Spans::from(msg));
    text.patch_style(style);
    let help_message = Paragraph::new(text);
    f.render_widget(help_message, chunks[0]);

    let input = Paragraph::new(app.input.as_ref())
        .style(match app.input_mode {
            InputMode::Normal => Style::default(),
            InputMode::Editing => Style::default().fg(Color::Yellow),
        })
        .block(Block::default().borders(Borders::ALL).title("Input"));
    f.render_widget(input, chunks[1]);
    match app.input_mode {
        InputMode::Normal =>
            // Hide the cursor. `Frame` does this by default, so we don't need to do anything here
            {}

        InputMode::Editing => {
            // Make the cursor visible and ask tui-rs to put it at the specified coordinates after rendering
            f.set_cursor(
                // Put cursor past the end of the input text
                chunks[1].x + app.input.width() as u16 + 1,
                // Move one line down, from the border to the input line
                chunks[1].y + 1,
            )
        }
    }

    let messages: Vec<ListItem> = app
        .messages
        .read()
        .unwrap()
        .iter()
        .map(|message| {
            let content = vec![Spans::from(Span::raw(format!(
                "[{} - {}]: {}",
                message.timestamp().unwrap().format("%d-%m-%y %H:%M"),
                message.nick(),
                message.message()
            )))];
            ListItem::new(content)
        })
        .collect();
    let messages = List::new(messages).block(Block::default().borders(Borders::ALL).title("Chat"));
    f.render_widget(messages, chunks[2]);
}
