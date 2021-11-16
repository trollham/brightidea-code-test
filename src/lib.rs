pub mod api;

use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Weak,
    },
};

use futures::{SinkExt, StreamExt, TryFutureExt};
use tokio::{
    fs::File,
    io::{AsyncWriteExt, BufWriter},
    sync::{mpsc, RwLock},
};
use tokio_stream::wrappers::UnboundedReceiverStream;
use warp::ws::{Message, WebSocket};

/// Our global unique user id counter.
static NEXT_USER_ID: AtomicUsize = AtomicUsize::new(1);

/// Our state of currently connected users.
///
/// - Key is their id
/// - Value is a sender of `warp::ws::Message`
pub type Users = Arc<RwLock<HashMap<usize, mpsc::UnboundedSender<Message>>>>;
pub type ChatRooms = Arc<RwLock<HashMap<String, Weak<ChatRoom>>>>;

#[derive(Debug)]
pub struct ChatRoom {
    pub name: String,
    pub users: Users,
    logging_tx: mpsc::UnboundedSender<String>,
    cancellation_tx: mpsc::UnboundedSender<()>,
}

impl ChatRoom {
    pub async fn new(name: String, users: Users) -> ChatRoom {
        // set up communication channels
        let (tx, rx) = mpsc::unbounded_channel::<String>();
        let mut rx = UnboundedReceiverStream::new(rx);
        let (cancellation_tx, mut cancellation_rx) = mpsc::unbounded_channel::<()>();

        let file_name = format!(
            "{}_{}.log",
            name,
            humantime::format_rfc3339(std::time::SystemTime::now())
        );

        // This task handles writing to the log using a BufWriter
        tokio::task::spawn(async move {
            let file = File::create(&file_name).await.unwrap(); // TODO error handle
            let mut log_writer = BufWriter::new(file);
            loop {
                tokio::select! {
                    Some(message) = rx.next() => {
                        if let Err(e) = log_writer.write_all(format!("{}\n", message).as_bytes()).await {
                            eprintln!("Error writing message: {:?}", e);
                        }
                    },
                    Some(_) = cancellation_rx.recv() => {
                        break;
                    }
                }
            }
            if let Err(e) = log_writer.flush().await {
                eprintln!(
                    "Failed to write log for channel. Name: {}, Error: {}",
                    file_name, e
                );
            }
        });

        ChatRoom {
            name,
            users,
            logging_tx: tx,
            cancellation_tx,
        }
    }

    pub fn log_message(&self, msg: &str, user_id: usize) {
        if self
            .logging_tx
            .send(format!("Channel {}, user {}: {}", &self.name, user_id, msg))
            .is_err()
        {
            eprintln!(
                "Failed to log message. Channel: {}, user: {}, message: {}",
                self.name, user_id, msg
            );
        }
    }

    pub fn broadcast(&self, msg: &str) {
        
    }
}

impl Drop for ChatRoom {
    fn drop(&mut self) {
        if self.cancellation_tx.send(()).is_err() {
            eprintln!("Failed to send cancel notice to logging task, log may be incomplete. Channel: {}", self.name);
        }
        eprintln!("Channel destroyed: {}", self.name);
    }
}

async fn get_room(room_name: &str, rooms: ChatRooms) -> Arc<ChatRoom> {
    rooms
        .write()
        .await
        .retain(|_, room_ptr| room_ptr.strong_count() > 0); // lazily remove closed channels

    let maybe_room = if let Some(c) = rooms.read().await.get(room_name) {
        c.upgrade()
    } else {
        None
    };
    match maybe_room {
        Some(room) => {
            eprintln!("channel reused: {}", room_name);
            room
        }
        None => {
            let room = Arc::new(ChatRoom::new(room_name.to_owned(), Users::default()).await);
            rooms
                .write()
                .await
                .insert(room_name.to_owned(), Arc::downgrade(&room));
            eprintln!("channel created: {}", room_name);
            room
        }
    }
}

async fn user_connected(ws: WebSocket, room: Arc<ChatRoom>) {
    // Use a counter to assign a new unique ID for this user.
    let my_id = NEXT_USER_ID.fetch_add(1, Ordering::Relaxed);

    eprintln!("new chat user: {}", my_id);

    // Split the socket into a sender and receive of messages.
    let (mut user_ws_tx, mut user_ws_rx) = ws.split();

    // Use an unbounded channel to handle buffering and flushing of messages
    // to the websocket...
    let (tx, rx) = mpsc::unbounded_channel();
    let mut rx = UnboundedReceiverStream::new(rx);

    tokio::task::spawn(async move {
        while let Some(message) = rx.next().await {
            user_ws_tx
                .send(message)
                .unwrap_or_else(|e| {
                    eprintln!("websocket send error: {}", e);
                })
                .await;
        }
    });

    // Save the sender in our list of connected users.
    room.users.write().await.insert(my_id, tx);

    // Return a `Future` that is basically a state machine managing
    // this specific user's connection.

    // Every time the user sends a message, broadcast it to
    // all other users...
    while let Some(result) = user_ws_rx.next().await {
        let msg = match result {
            Ok(msg) => msg,
            Err(e) => {
                eprintln!("websocket error(uid={}): {}", my_id, e);
                break;
            }
        };
        // Skip any non-Text messages, logging any errors
        if msg.is_text() {
            match msg.to_str() {
                Ok(s) => {
                    room.log_message(s, my_id);
                    user_message(my_id, s, &room.users).await;
                }
                Err(_) => {
                    room.log_message("!!!ATTEMPTED TO SEND NON-TEXT MESSAGE!!!", my_id);
                }
            }
        }
    }

    // user_ws_rx stream will keep processing as long as the user stays
    // connected. Once they disconnect, then...
    user_disconnected(my_id, &room.users).await;
}

async fn user_message(my_id: usize, msg: &str, users: &Users) {
    let new_msg = format!("<User#{}>: {}", my_id, msg);

    // New message from this user, send it to everyone else (except same uid)...
    for (&uid, tx) in users.read().await.iter() {
        if my_id != uid {
            if let Err(_disconnected) = tx.send(Message::text(new_msg.clone())) {
                // The tx is disconnected, our `user_disconnected` code
                // should be happening in another task, nothing more to
                // do here.
            }
        }
    }
}

async fn user_disconnected(my_id: usize, users: &Users) {
    eprintln!("good bye user: {}", my_id);

    // Stream closed up, so remove from the user list
    users.write().await.remove(&my_id);
}
