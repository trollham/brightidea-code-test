use std::{collections::HashMap, convert::Infallible, sync::{Arc, Weak, atomic::{AtomicUsize, Ordering}}};

use futures::{SinkExt, StreamExt, TryFutureExt};
use tokio::sync::{RwLock, mpsc};
use tokio_stream::wrappers::UnboundedReceiverStream;
use warp::ws::{Message, WebSocket};


/// Our global unique user id counter.
static NEXT_USER_ID: AtomicUsize = AtomicUsize::new(1);

/// Our state of currently connected users.
///
/// - Key is their id
/// - Value is a sender of `warp::ws::Message`
pub type Users = Arc<RwLock<HashMap<usize, mpsc::UnboundedSender<Message>>>>;
pub type Channels = Arc<RwLock<HashMap<String, Weak<Channel>>>>;

pub struct Channel {
    users: Users,
}

pub async fn upgrade(
    channel_name: String,
    ws: warp::ws::Ws,
    channels: Channels,
) -> Result<impl warp::Reply, Infallible> {
    // This will call our function if the handshake succeeds.
    let channel = get_channel(&channel_name, channels).await;
    if channel.is_none() {
        eprintln!("channel has been deleted: {}", channel_name);
    }
    let channel = channel.unwrap();
    Ok(ws.on_upgrade(move |socket| user_connected(socket, channel)))
}

async fn get_channel(channel_name: &str, channels: Channels) -> Option<Arc<Channel>> {
    if let Some(c) = channels.read().await.get(channel_name) {
        eprintln!("channel reused: {}", channel_name);
        return c.upgrade();
    } 
    let c = Arc::new(Channel {
        users: Users::default(),
    });
    channels
        .write()
        .await
        .insert(channel_name.to_owned(), Arc::downgrade(&c));
    eprintln!("channel created: {}", channel_name);
    Some(c)
    
}

async fn user_connected(ws: WebSocket, channel: Arc<Channel>) {
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
    channel.users.write().await.insert(my_id, tx);

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
        user_message(my_id, msg, &channel.users).await;
    }

    // user_ws_rx stream will keep processing as long as the user stays
    // connected. Once they disconnect, then...
    user_disconnected(my_id, &channel.users).await;
}

async fn user_message(my_id: usize, msg: Message, users: &Users) {
    // Skip any non-Text messages...
    let msg = if let Ok(s) = msg.to_str() {
        s
    } else {
        return;
    };

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