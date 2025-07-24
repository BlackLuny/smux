use criterion::{Criterion, Throughput, black_box, criterion_group, criterion_main};
use smux::{Config, Session};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::runtime::Runtime;

// Match Go benchmark: 128KB chunks
const GO_CHUNK_SIZE: usize = 128 * 1024; // 128KB to match Go benchmark

async fn create_tcp_session_pair() -> (Arc<Session<TcpStream>>, Arc<Session<TcpStream>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server_handle = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let config = Config::default();
        Session::server(stream, config).await.unwrap()
    });

    let client_stream = TcpStream::connect(addr).await.unwrap();
    let config = Config::default();
    let client_session = Session::client(client_stream, config).await.unwrap();
    let server_session = server_handle.await.unwrap();

    (Arc::new(client_session), Arc::new(server_session))
}

// Create TCP connection pair for benchmarking
async fn create_tcp_connection_pair() -> (TcpStream, TcpStream) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server_handle = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        stream
    });

    let client_stream = TcpStream::connect(addr).await.unwrap();
    let server_stream = server_handle.await.unwrap();

    (client_stream, server_stream)
}

// Go-style benchmark: BenchmarkConnSmux equivalent
fn bench_conn_smux(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("go_style_smux");
    let n = 10000;

    // Match Go benchmark settings
    group.sample_size(10);
    group.measurement_time(std::time::Duration::from_secs(10));
    group.throughput(Throughput::Bytes((GO_CHUNK_SIZE as u64) * n as u64));

    // Create persistent TCP sessions once for all iterations
    let (client_session, server_session) = rt.block_on(create_tcp_session_pair());

    group.bench_function("BenchmarkConnSmux", |b| {
        b.iter_batched(
            || {
                // Setup: create new streams for each iteration
                rt.block_on(async {
                    let client_stream = client_session.clone().open_stream().await.unwrap();
                    let server_stream = server_session
                        .clone()
                        .accept_stream()
                        .await
                        .unwrap()
                        .unwrap();
                    (client_stream, server_stream)
                })
            },
            |(mut client_stream, mut server_stream)| {
                // Benchmark: use the pre-created streams
                rt.block_on(async {
                    let write_buf = vec![42u8; GO_CHUNK_SIZE];
                    let mut read_buf = vec![0u8; GO_CHUNK_SIZE];

                    let reader_handle = tokio::spawn(async move {
                        let mut total_read = 0;
                        while total_read < GO_CHUNK_SIZE * n {
                            match server_stream.read(&mut read_buf).await {
                                Ok(0) => break, // EOF
                                Ok(n) => total_read += n,
                                Err(_) => break,
                            }
                        }
                        total_read as u64
                    });

                    let writer_handle = tokio::spawn(async move {
                        for _i in 0..n {
                            client_stream.write_all(&write_buf).await.unwrap();
                            let _invalidate_buf = vec![43u8; GO_CHUNK_SIZE];
                        }
                        client_stream.flush().await.unwrap();
                        client_stream.close().await.unwrap();
                        GO_CHUNK_SIZE as u64 * n as u64
                    });

                    let (received, _sent) = tokio::join!(reader_handle, writer_handle);
                    black_box(received.unwrap())
                })
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.finish();
}

// Go-style benchmark: BenchmarkConnTCP equivalent
fn bench_conn_tcp(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("go_style_tcp");
    let n = 10000;

    group.sample_size(10);
    group.measurement_time(std::time::Duration::from_secs(10));
    group.throughput(Throughput::Bytes((GO_CHUNK_SIZE as u64) * n as u64));

    group.bench_function("BenchmarkConnTCP", |b| {
        b.iter_batched(
            || {
                // Setup: create new TCP connection for each iteration
                rt.block_on(create_tcp_connection_pair())
            },
            |(mut client_stream, mut server_stream)| {
                // Benchmark: use the pre-created TCP connection
                rt.block_on(async {
                    let write_buf = vec![42u8; GO_CHUNK_SIZE];
                    let mut read_buf = vec![0u8; GO_CHUNK_SIZE];

                    let reader_handle = tokio::spawn(async move {
                        let mut total_read = 0;
                        while total_read < GO_CHUNK_SIZE * n {
                            match server_stream.read(&mut read_buf).await {
                                Ok(0) => break, // EOF
                                Ok(n) => total_read += n,
                                Err(_) => break,
                            }
                        }
                        total_read as u64
                    });

                    let writer_handle = tokio::spawn(async move {
                        for _i in 0..n {
                            client_stream.write_all(&write_buf).await.unwrap();
                            let _invalidate_buf = vec![43u8; GO_CHUNK_SIZE];
                        }
                        client_stream.flush().await.unwrap();
                        drop(client_stream);
                        GO_CHUNK_SIZE as u64 * n as u64
                    });

                    let (received, _sent) = tokio::join!(reader_handle, writer_handle);
                    black_box(received.unwrap())
                })
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.finish();
}

criterion_group!(benches, bench_conn_smux, bench_conn_tcp);
criterion_main!(benches);
