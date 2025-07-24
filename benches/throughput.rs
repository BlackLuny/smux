use criterion::{Criterion, Throughput, black_box, criterion_group, criterion_main};
use smux::{Config, Session};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

// Match Go benchmark: 128KB chunks
const CHUNK_SIZE: usize = 128 * 1024; // 128KB to match Go benchmark

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

async fn run_benchmark_routine<R, W>(
    mut reader: R,
    mut writer: W,
    write_buf: Vec<u8>,
    read_buf: Vec<u8>,
    n: usize,
) -> u64
where
    R: AsyncReadExt + Send + Unpin + 'static,
    W: AsyncWriteExt + Send + Unpin + 'static,
{
    let reader_handle = tokio::spawn(async move {
        let mut read_buf = read_buf;
        let mut total_read = 0;
        while total_read < CHUNK_SIZE * n {
            match reader.read(&mut read_buf).await {
                Ok(0) => break,
                Ok(bytes_read) => total_read += bytes_read,
                Err(_) => break,
            }
        }
        total_read as u64
    });

    let writer_handle = tokio::spawn(async move {
        for _i in 0..n {
            writer.write_all(&write_buf).await.unwrap();
        }
        writer.flush().await.unwrap();
        CHUNK_SIZE as u64 * n as u64
    });

    let (received, _sent) = tokio::join!(reader_handle, writer_handle);
    received.unwrap()
}

async fn run_smux_benchmark(
    mut server_stream: smux::Stream,
    mut client_stream: smux::Stream,
    write_buf: Vec<u8>,
    read_buf: Vec<u8>,
    n: usize,
) -> u64 {
    let reader_handle = tokio::spawn(async move {
        let mut read_buf = read_buf;
        let mut total_read = 0;
        while total_read < CHUNK_SIZE * n {
            match server_stream.read(&mut read_buf).await {
                Ok(0) => break,
                Ok(bytes_read) => total_read += bytes_read,
                Err(_) => break,
            }
        }
        total_read as u64
    });

    let writer_handle = tokio::spawn(async move {
        for _i in 0..n {
            client_stream.write_all(&write_buf).await.unwrap();
        }
        client_stream.flush().await.unwrap();
        client_stream.close().await.unwrap();
        CHUNK_SIZE as u64 * n as u64
    });

    let (received, _sent) = tokio::join!(reader_handle, writer_handle);
    received.unwrap()
}

fn bench_conn_smux(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut group = c.benchmark_group("smux");
    let n = 10000;

    group.sample_size(10);
    group.measurement_time(std::time::Duration::from_secs(10));
    group.throughput(Throughput::Bytes((CHUNK_SIZE as u64) * n as u64));

    let (client_session, server_session) = rt.block_on(create_tcp_session_pair());

    group.bench_function("BenchmarkConnSmux", |b| {
        b.iter_batched(
            || {
                rt.block_on(async {
                    let client_stream = client_session.clone().open_stream().await.unwrap();
                    let server_stream = server_session.clone().accept_stream().await.unwrap();
                    let write_buf = vec![42u8; CHUNK_SIZE];
                    let read_buf = vec![0u8; CHUNK_SIZE];
                    (client_stream, server_stream, write_buf, read_buf)
                })
            },
            |(client_stream, server_stream, write_buf, read_buf)| {
                rt.block_on(async {
                    let result =
                        run_smux_benchmark(server_stream, client_stream, write_buf, read_buf, n)
                            .await;
                    black_box(result)
                })
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.finish();
}

fn bench_conn_tcp(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut group = c.benchmark_group("raw_tcp");
    let n = 10000;

    group.sample_size(10);
    group.measurement_time(std::time::Duration::from_secs(10));
    group.throughput(Throughput::Bytes((CHUNK_SIZE as u64) * n as u64));

    group.bench_function("BenchmarkConnTCP", |b| {
        b.iter_batched(
            || {
                let (stream1, stream2) = rt.block_on(create_tcp_connection_pair());
                let write_buf = vec![42u8; CHUNK_SIZE];
                let read_buf = vec![0u8; CHUNK_SIZE];
                (stream1, stream2, write_buf, read_buf)
            },
            |(client_stream, server_stream, write_buf, read_buf)| {
                rt.block_on(async {
                    let result =
                        run_benchmark_routine(server_stream, client_stream, write_buf, read_buf, n)
                            .await;
                    black_box(result)
                })
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.finish();
}

criterion_group!(benches, bench_conn_smux, bench_conn_tcp);
criterion_main!(benches);
