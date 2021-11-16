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

use brightidea_test::{ChatRooms, api};

#[tokio::main]
async fn main() {
    pretty_env_logger::init();

    // Keep track of all channels and their respective users
    let rooms = ChatRooms::default();

    // let index = warp::path::end().map(|| warp::reply::html(INDEX_HTML));
    let routes = api::build_filters(rooms);

    warp::serve(routes).run(([127, 0, 0, 1], 3030)).await;
}
