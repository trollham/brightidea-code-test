use brightidea_test::{ChatRoom, ChatRooms, Users};
use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};

pub fn criterion_benchmark(c: &mut Criterion) {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let chatroom = ChatRoom::new("benchmark_test".to_owned(), Users::default()).await;

            c.bench_function("log hello, world", |b| {
                b.iter(|| chatroom.log_message(&"hello_world".to_string(), 0))
            });
        });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
