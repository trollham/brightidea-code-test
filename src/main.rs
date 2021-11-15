// #![deny(warnings)]

// Start from this sample chat server https://github.com/seanmonstar/warp/blob/master/examples/websockets_chat.rs

// Implement 2 new features:
// * Add basic support for multiple chat rooms. user should only receive messages sent to the same chat room. you can use URL to identify chat rooms (e.g. ws://localhost/chat/room1)
//      - The core functionality is to use URL to identify different chat room, sending message to the same
//      - URL should only broadcast to other users connected to the same URL.
//      - Some example of support to add: users don’t need to join/leave chat room, or cleanup idle chat rooms. Or anything else you can think of
// * Write transcript of each chat room to local file. design it in a way that would be able to scale for thousands of messages per second.

// Write at least 1 test.
// Feel free to organize the code however you see fit

use std::convert::Infallible;

use brightidea_test::{upgrade_connection, Channels};
use warp::Filter;

// GET /{room: str} -> index html to join room
fn room() -> impl warp::Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    warp::path!(String).map(|_channel| warp::reply::html(INDEX_HTML))
}

fn with_channels(
    channels: Channels,
) -> impl warp::Filter<Extract = (Channels,), Error = Infallible> + Clone {
    warp::any().map(move || channels.clone())
}

// GET /chat/{room: str}-> websocket upgrade
fn ws_upgrade(
    channels: Channels,
) -> impl warp::Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    warp::path!("chat" / String)
        // The `ws()` filter will prepare Websocket handshake...
        .and(warp::ws())
        .and(with_channels(channels))
        .and_then(upgrade_connection)
}

#[tokio::main]
async fn main() {
    pretty_env_logger::init();

    // Keep track of all channels and their respective users
    let channels = Channels::default();

    // let index = warp::path::end().map(|| warp::reply::html(INDEX_HTML));
    let routes = room().or(ws_upgrade(channels));

    warp::serve(routes).run(([127, 0, 0, 1], 3030)).await;
}

static INDEX_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
    <head>
        <title>Warp Chat</title>
    </head>
    <body>
        <h1>Warp chat</h1>
        <div id="chat">
            <p><em>Connecting...</em></p>
        </div>
        <input type="text" id="text" />
        <button type="button" id="send">Send</button>
        <script type="text/javascript">
        const chat = document.getElementById('chat');
        const text = document.getElementById('text');
        const room_name = location.pathname.split('/');
        console.log(room_name);
        const uri = 'ws://' + location.host + '/chat/' + room_name[1]; 
        const ws = new WebSocket(uri);
        function message(data) {
            const line = document.createElement('p');
            line.innerText = data;
            chat.appendChild(line);
        }
        ws.onopen = function() {
            chat.innerHTML = '<p><em>Connected!</em></p>';
        };
        ws.onmessage = function(msg) {
            message(msg.data);
        };
        ws.onclose = function() {
            chat.getElementsByTagName('em')[0].innerText = 'Disconnected!';
        };
        send.onclick = function() {
            const msg = text.value;
            ws.send(msg);
            text.value = '';
            message('<You>: ' + msg);
        };
        </script>
    </body>
</html>
"#;

#[cfg(test)]
mod tests {
    use brightidea_test::Channels;

    use crate::{room, ws_upgrade, INDEX_HTML};

    #[tokio::test]
    async fn chat_endpoint() {
        let filter = room();
        let ok_reply = warp::test::request()
            .path("/test_room")
            .reply(&filter)
            .await;

        assert_eq!(ok_reply.status(), 200);
        assert_eq!(ok_reply.body(), INDEX_HTML);

        let no_room_provided = warp::test::request().path("/").reply(&filter).await;
        assert_eq!(no_room_provided.status(), 404);

        let too_many_rooms = warp::test::request()
            .path("/test/room")
            .reply(&filter)
            .await;
        assert_eq!(too_many_rooms.status(), 404);
    }

    #[tokio::test]
    async fn chat_upgrade_endpoint() {
        let channels = Channels::default();
        let filter = ws_upgrade(channels.clone());

        let ok_reply = warp::test::ws()
            .path("/chat/test_room")
            .handshake(filter)
            .await;
        assert!(ok_reply.is_ok());

        let channel_read_lock = channels.read().await;
        assert_eq!(channel_read_lock.len(), 1);

        let test_room_channel = channel_read_lock.get("test_room");
        assert!(test_room_channel.is_some());

        let test_room_channel = test_room_channel.unwrap().upgrade().unwrap();
        assert_eq!(test_room_channel.name, "test_room");
        assert_eq!(test_room_channel.users.read().await.len(), 1);

        // Fail test
        let filter = ws_upgrade(channels.clone());
        let no_room = warp::test::ws()
            .path("/chat")
            .handshake(filter)
            .await;
        assert!(!no_room.is_ok());
    }
}
