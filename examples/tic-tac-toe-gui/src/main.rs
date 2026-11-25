use eframe::egui;
use serde::{Deserialize, Serialize};
use std::str::from_utf8;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::task;

use tokio::sync::mpsc;
use waku::{
    general::pubsubtopic::PubsubTopic, waku_new, Encoding, Initialized, LibwakuResponse, Running,
    WakuContentTopic, WakuEvent, WakuMessage, WakuNodeConfig, WakuNodeHandle,
};

#[derive(Serialize, Deserialize, PartialEq, Debug, Copy, Clone)]
enum Player {
    X,
    O,
}

#[derive(Serialize, Deserialize, Clone)]
struct GameState {
    board: [[Option<Player>; 3]; 3],
    current_turn: Player,
    moves_left: usize,
}

struct TicTacToeApp<State> {
    game_state: Arc<Mutex<GameState>>,
    waku: WakuNodeHandle<State>,
    game_topic: PubsubTopic,
    tx: mpsc::Sender<String>,    // Sender to send `msg` to main thread
    player_role: Option<Player>, // Store the player's role (X or O)
}

impl TicTacToeApp<Initialized> {
    fn new(
        waku: WakuNodeHandle<Initialized>,
        game_topic: PubsubTopic,
        game_state: Arc<Mutex<GameState>>,
        tx: mpsc::Sender<String>,
    ) -> Self {
        Self {
            game_state,
            waku,
            game_topic,
            tx,
            player_role: None,
        }
    }

    async fn start(self) -> TicTacToeApp<Running> {
        let tx_clone = self.tx.clone();
        let game_content_topic =
            WakuContentTopic::new("waku", "2", "tictactoegame", Encoding::Proto);

        let my_closure = move |response| {
            if let LibwakuResponse::Success(v) = response {
                let event: WakuEvent =
                    serde_json::from_str(v.unwrap().as_str()).expect("Parsing event to succeed");

                match event {
                    WakuEvent::WakuMessage(evt) => {
                        let message = evt.waku_message;
                        // Filter: only process messages for our game content topic
                        if message.content_topic != game_content_topic {
                            return; // Skip messages from other apps
                        }
                        let payload = message.payload.to_vec();
                        match from_utf8(&payload) {
                            Ok(msg) => {
                                //  Lock succeeded, proceed to send the message
                                if tx_clone.blocking_send(msg.to_string()).is_err() {
                                    eprintln!("Failed to send message to async task");
                                }
                            }
                            Err(e) => {
                                eprintln!("Failed to decode payload as UTF-8: {}", e);
                                // Handle the error as needed, or just log and skip
                            }
                        }
                    }
                    WakuEvent::RelayTopicHealthChange(_evt) => {
                        // dbg!("Relay topic change evt", evt);
                    }
                    WakuEvent::ConnectionChange(_evt) => {
                        // dbg!("Conn change evt", evt);
                    }
                    WakuEvent::Unrecognized(err) => panic!("Unrecognized waku event: {:?}", err),
                    _ => panic!("event case not expected"),
                };
            }
        };

        // Establish a closure that handles the incoming messages
        self.waku
            .set_event_callback(my_closure)
            .expect("set event call back working");

        // Start the waku node
        let waku = self.waku.start().await.expect("waku should start");

        // Subscribe to desired topic using the relay protocol
        waku.relay_subscribe(&self.game_topic)
            .await
            .expect("waku should subscribe");

        // Example filter subscription. This is needed in edge nodes (resource-restricted devices)
        // Nodes usually use either relay or lightpush/filter protocols

        // let ctopic = WakuContentTopic::new("waku", "2", "tictactoegame", Encoding::Proto);
        // let content_topics = vec![ctopic];
        // waku.filter_subscribe(&self.game_topic, content_topics).await.expect("waku should subscribe");

        // End filter example ----------------------------------------

        // Example to establish direct connection to a well-known node

        // Connect to hard-coded node
        // let target_node_multi_addr =
        //     "/ip4/159.223.242.94/tcp/30303/p2p/16Uiu2HAmAUdrQ3uwzuE4Gy4D56hX6uLKEeerJAnhKEHZ3DxF1EfT"
        //     // "/dns4/store-01.do-ams3.status.prod.status.im/tcp/30303/p2p/16Uiu2HAmAUdrQ3uwzuE4Gy4D56hX6uLKEeerJAnhKEHZ3DxF1EfT"
        //     // "/ip4/24.144.78.119/tcp/30303/p2p/16Uiu2HAm3xVDaz6SRJ6kErwC21zBJEZjavVXg7VSkoWzaV1aMA3F"
        //     .parse::<Multiaddr>().expect("parse multiaddress");

        // self.waku.connect(&target_node_multi_addr, None)
        //      .expect("waku should connect to other node");

        // End example direct connection

        TicTacToeApp {
            game_state: self.game_state,
            waku,
            game_topic: self.game_topic,
            tx: self.tx,
            player_role: self.player_role,
        }
    }
}

impl TicTacToeApp<Running> {
    async fn send_game_state(&self, game_state: &GameState) {
        let serialized_game_state = serde_json::to_string(game_state).unwrap();
        let content_topic = WakuContentTopic::new("waku", "2", "tictactoegame", Encoding::Proto);

        let message = WakuMessage::new(&serialized_game_state, content_topic, 0, Vec::new(), false);

        if let Ok(msg_hash) = self
            .waku
            .relay_publish_message(&message, &self.game_topic, None)
            .await
        {
            dbg!(format!("message hash published: {}", msg_hash));
        }

        // Example lightpush publish message. This is needed in edge nodes (resource-restricted devices)
        // Nodes usually use either relay or lightpush/filter protocols
        //
        // let msg_hash_ret = self.waku.lightpush_publish_message(&message, &self.game_topic).await;
        // match msg_hash_ret {
        //     Ok(msg_hash) => println!("Published message hash {:?}", msg_hash.to_string()),
        //     Err(error) => println!("Failed to publish with lightpush: {}", error)
        // }
        // End example lightpush publish message
    }

    fn make_move(&mut self, row: usize, col: usize) {
        if let Ok(mut game_state) = self.game_state.try_lock() {
            if let Some(my_role) = self.player_role {
                if game_state.current_turn != my_role {
                    return; // skip click if not my turn
                }
            }

            if game_state.board[row][col].is_none() && game_state.moves_left > 0 {
                game_state.board[row][col] = Some(game_state.current_turn);
                game_state.moves_left -= 1;

                if let Some(winner) = self.check_winner(&game_state) {
                    game_state.current_turn = winner;
                } else {
                    game_state.current_turn = match game_state.current_turn {
                        Player::X => Player::O,
                        Player::O => Player::X,
                    };
                }

                // Call the async function in a blocking context
                task::block_in_place(|| {
                    // Obtain the current runtime handle
                    let handle = tokio::runtime::Handle::current();

                    // Block on the async function
                    handle.block_on(async {
                        // Assuming `self` is available in the current context
                        self.send_game_state(&game_state).await;
                    });
                });
            }
        }
    }

    fn check_winner(&self, game_state: &GameState) -> Option<Player> {
        // Check rows, columns, and diagonals
        for i in 0..3 {
            if game_state.board[i][0] == game_state.board[i][1]
                && game_state.board[i][1] == game_state.board[i][2]
            {
                if let Some(player) = game_state.board[i][0] {
                    return Some(player);
                }
            }
            if game_state.board[0][i] == game_state.board[1][i]
                && game_state.board[1][i] == game_state.board[2][i]
            {
                if let Some(player) = game_state.board[0][i] {
                    return Some(player);
                }
            }
        }
        if game_state.board[0][0] == game_state.board[1][1]
            && game_state.board[1][1] == game_state.board[2][2]
        {
            if let Some(player) = game_state.board[0][0] {
                return Some(player);
            }
        }
        if game_state.board[0][2] == game_state.board[1][1]
            && game_state.board[1][1] == game_state.board[2][0]
        {
            if let Some(player) = game_state.board[0][2] {
                return Some(player);
            }
        }
        None
    }

    fn reset_game(&mut self) {
        self.game_state = Arc::new(Mutex::new(GameState {
            board: [[None; 3]; 3],
            current_turn: Player::X,
            moves_left: 9,
        }));
        self.player_role = None
    }
}

impl eframe::App for TicTacToeApp<Running> {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Request a repaint every second
        ctx.request_repaint_after(Duration::from_secs(1));

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Tic-Tac-Toe");

            // If the player hasn't selected a role, show the role selection buttons
            if self.player_role.is_none() {
                ui.label("Select your role:");

                if ui.button("Play as X").clicked() {
                    self.player_role = Some(Player::X);
                }

                if ui.button("Play as O").clicked() {
                    self.player_role = Some(Player::O);
                    // Player O waits for Player X to make the first move
                    // No need to change current_turn as it's already X
                }

                return; // Exit early until a role is selected
            }

            let player_role = self.player_role.unwrap(); // Safe to unwrap because we've ensured it's Some

            // Main game UI
            ui.label(format!("You are playing as: {:?}", player_role));

            // Draw the game board and handle the game state
            let board_size = ui.available_size();
            let cell_size = board_size.x / 4.0;

            ui.horizontal(|ui| {
                for row in 0..3 {
                    ui.vertical(|ui| {
                        for col in 0..3 {
                            let label;
                            {
                                if let Ok(game_state) = self.game_state.try_lock() {
                                    label = match game_state.board[row][col] {
                                        Some(Player::X) => "X",
                                        Some(Player::O) => "O",
                                        None => "-",
                                    };
                                } else {
                                    label = "#";
                                }
                            }

                            let button = ui.add(
                                egui::Button::new(label)
                                    .min_size(egui::vec2(cell_size, cell_size))
                                    .sense(egui::Sense::click()),
                            );

                            if button.clicked() {
                                self.make_move(row, col);
                            }
                        }
                    });
                    if row < 2 {
                        ui.add_space(4.0);
                    }
                }
            });

            if let Ok(game_state) = self.game_state.try_lock() {
                if let Some(winner) = self.check_winner(&game_state) {
                    ui.label(format!(
                        "Player {} wins!",
                        match winner {
                            Player::X => "X",
                            Player::O => "O",
                        }
                    ));
                } else if game_state.moves_left == 0 {
                    ui.label("It's a tie!");
                } else {
                    ui.label(format!(
                        "Player {}'s turn",
                        match game_state.current_turn {
                            Player::X => "X",
                            Player::O => "O",
                        }
                    ));
                }
            }

            if ui.add(egui::Button::new("Restart Game")).clicked() {
                self.reset_game();
            }
        });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // TODO: implement the cleanup an proper stop of waku node
    }
}

#[tokio::main]
async fn main() -> eframe::Result<()> {
    let (tx, mut rx) = mpsc::channel::<String>(3200); // Channel to communicate between threads

    let game_topic = PubsubTopic::new("/waku/2/rs/16/32");
    // Create a Waku instance
    let waku = waku_new(Some(WakuNodeConfig {
        tcp_port: Some(60010),
        cluster_id: Some(16),
        shards: vec![1, 32, 64, 128, 256],
        // node_key: Some(SecretKey::from_str("2fc0515879e52b7b73297cfd6ab3abf7c344ef84b7a90ff6f4cc19e05a198027").unwrap()),
        max_message_size: Some("1024KiB".to_string()),
        relay_topics: vec![String::from(&game_topic)],
        log_level: Some("FATAL"), // Supported: TRACE, DEBUG, INFO, NOTICE, WARN, ERROR or FATAL

        keep_alive: Some(true),

        // Discovery
        dns_discovery: Some(true),
        dns_discovery_url: Some("enrtree://AMOJVZX4V6EXP7NTJPMAYJYST2QP6AJXYW76IU6VGJS7UVSNDYZG4@boot.prod.status.nodes.status.im"),
        // discv5_discovery: Some(true),
        // discv5_udp_port: Some(9001),
        // discv5_enr_auto_update: Some(false),

        ..Default::default()
    })).await
    .expect("should instantiate");

    let game_state = GameState {
        board: [[None; 3]; 3],
        current_turn: Player::X,
        moves_left: 9,
    };
    let shared_state = Arc::new(Mutex::new(game_state));

    let clone = shared_state.clone();
    let app = TicTacToeApp::new(waku, game_topic, clone, tx);

    let app = app.start().await;

    let clone = shared_state.clone();
    // Listen for messages in the main thread
    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            // println!("MSG received: {}", msg);
            // Handle the received message, e.g., update the UI or game state
            if let Ok(parsed_value) = serde_json::from_str::<GameState>(&msg) {
                if let Ok(mut unclocked_game_state) = clone.lock() {
                    *unclocked_game_state = parsed_value;
                }
            } else {
                eprintln!("Failed to parse JSON");
            }
        }
    });

    eframe::run_native(
        "Tic-Tac-Toe Multiplayer via Waku",
        eframe::NativeOptions {
            initial_window_size: Some(egui::vec2(400.0, 400.0)),
            ..Default::default()
        },
        Box::new(|_cc| Box::new(app)),
    )?;

    Ok(())
}
