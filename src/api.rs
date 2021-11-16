use std::convert::Infallible;

use warp::Filter;

use crate::{ChatRooms, get_room, user_connected};

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

// GET /{room: str} -> index html to join room
fn room() -> impl warp::Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    warp::path!(String).map(|_| warp::reply::html(INDEX_HTML))
}

fn with_rooms(
    rooms: ChatRooms,
) -> impl warp::Filter<Extract = (ChatRooms,), Error = Infallible> + Clone {
    warp::any().map(move || rooms.clone())
}

async fn upgrade_connection(
    room_name: String,
    ws: warp::ws::Ws,
    rooms: ChatRooms,
) -> Result<impl warp::Reply, Infallible> {
    // This will call our function if the handshake succeeds.
    let channel = get_room(&room_name, rooms).await;
    Ok(ws.on_upgrade(move |socket| user_connected(socket, channel)))
}

// GET /chat/{room: str}-> websocket upgrade
fn ws_upgrade(
    rooms: ChatRooms,
) -> impl warp::Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    warp::path!("chat" / String)
        // The `ws()` filter will prepare Websocket handshake...
        .and(warp::ws())
        .and(with_rooms(rooms))
        .and_then(upgrade_connection)
}

pub fn build_filters(rooms: ChatRooms) -> impl warp::Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    room().or(ws_upgrade(rooms))
}

#[cfg(test)]
mod tests {
    use crate::{ChatRooms, api::{INDEX_HTML, room, ws_upgrade}};

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
        let channels = ChatRooms::default();
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
