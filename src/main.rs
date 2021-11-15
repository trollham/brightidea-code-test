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

use brightidea_test::{Channels, upgrade};
use warp::Filter;

#[tokio::main]
async fn main() {
    pretty_env_logger::init();

    // Keep track of all connected users, key is usize, value
    // is a websocket sender.
    let channels = Channels::default();

    let channels = warp::any().map(move || channels.clone());

    // GET /chat -> websocket upgrade
    let chat = warp::path!("chat" / String)
        // The `ws()` filter will prepare Websocket handshake...
        .and(warp::ws())
        .and(channels)
        .and_then(upgrade);

    // GET / -> index html
    // let index = warp::path::end().map(|| warp::reply::html(INDEX_HTML));
    let chat_room = warp::path!("room" / String).map(|_channel| warp::reply::html(INDEX_HTML));

    let routes = chat_room.or(chat);

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
        const uri = 'ws://' + location.host + '/chat/' + room_name[2]; 
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
