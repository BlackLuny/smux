use criterion::{Criterion, Throughput, black_box, criterion_group, criterion_main};
use smux::{Config, Session};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::runtime::Runtime;

// Match Go benchmark: 128KB chunks
const GO_CHUNK_SIZE: usize = 128 * 1024; // 128KB to match Go benchmark
const CHUNK_SIZE: usize = 64 * 1024; // Our original 64KB for other tests

const QUICK_SMALL: usize = 1024 * 1024; // 1MB

async fn create_session_pair() -> (
    Session<tokio::io::DuplexStream>,
    Session<tokio::io::DuplexStream>,
) {
    let (client_stream, server_stream) = tokio::io::duplex(8 * 1024 * 1024); // 8MB buffer
    let config = Config::default();
    let client_session = Session::client(client_stream, config.clone())
        .await
        .unwrap();
    let server_session = Session::server(server_stream, config).await.unwrap();
    (client_session, server_session)
}

// Go-style benchmark: single operation throughput test
async fn go_style_benchmark_smux() -> u64 {
    let (client_session, server_session) = create_session_pair().await;

    // Open single stream pair like Go benchmark
    let mut client_stream = client_session.open_stream().await.unwrap();
    let mut server_stream = server_session.accept_stream().await.unwrap().unwrap();

    // 128KB buffer like Go benchmark
    let write_buf = vec![42u8; GO_CHUNK_SIZE];
    let mut read_buf = vec![0u8; GO_CHUNK_SIZE];

    let reader_handle = tokio::spawn(async move {
        let mut total_read = 0;
        while total_read < GO_CHUNK_SIZE {
            match server_stream.read(&mut read_buf[total_read..]).await {
                Ok(0) => break, // EOF
                Ok(n) => total_read += n,
                Err(_) => break,
            }
        }
        total_read as u64
    });

    let writer_handle = tokio::spawn(async move {
        client_stream.write_all(&write_buf).await.unwrap();
        client_stream.flush().await.unwrap();
        let _ = client_stream.close().await; // Close to signal EOF

        // Invalidate cache like Go benchmark (simulate with new allocation)
        let _invalidate_buf = vec![43u8; GO_CHUNK_SIZE];
        GO_CHUNK_SIZE as u64
    });

    let (received, _sent) = tokio::join!(reader_handle, writer_handle);
    received.unwrap()
}

// Direct TCP-style test for comparison (using duplex stream as "TCP")
async fn go_style_benchmark_tcp() -> u64 {
    let (mut client_stream, mut server_stream) = tokio::io::duplex(1024 * 1024);

    let write_buf = vec![42u8; GO_CHUNK_SIZE];
    let mut read_buf = vec![0u8; GO_CHUNK_SIZE];

    let reader_handle = tokio::spawn(async move {
        let mut total_read = 0;
        while total_read < GO_CHUNK_SIZE {
            match server_stream.read(&mut read_buf[total_read..]).await {
                Ok(0) => break,
                Ok(n) => total_read += n,
                Err(_) => break,
            }
        }
        total_read as u64
    });

    let writer_handle = tokio::spawn(async move {
        client_stream.write_all(&write_buf).await.unwrap();
        client_stream.flush().await.unwrap();
        drop(client_stream); // Close to signal EOF

        // Invalidate cache
        let _invalidate_buf = vec![43u8; GO_CHUNK_SIZE];
        GO_CHUNK_SIZE as u64
    });

    let (received, _sent) = tokio::join!(reader_handle, writer_handle);
    received.unwrap()
}

// Simplified single-operation benchmark for direct comparison with Go
async fn simple_throughput_test(data_size: usize) -> u64 {
    let (client_session, server_session) = create_session_pair().await;
    let test_data = vec![42u8; CHUNK_SIZE];

    let client_stream = client_session.open_stream().await.unwrap();
    let server_stream = server_session.accept_stream().await.unwrap().unwrap();

    let send_handle = {
        let mut client_stream = client_stream;
        let test_data = test_data.clone();
        tokio::spawn(async move {
            let mut bytes_sent = 0;
            while bytes_sent < data_size {
                let chunk_size = std::cmp::min(CHUNK_SIZE, data_size - bytes_sent);
                if client_stream
                    .write_all(&test_data[..chunk_size])
                    .await
                    .is_err()
                {
                    break;
                }
                bytes_sent += chunk_size;
            }
            let _ = client_stream.close().await;
            bytes_sent
        })
    };

    let recv_handle = {
        let mut server_stream = server_stream;
        tokio::spawn(async move {
            let mut buffer = vec![0u8; CHUNK_SIZE];
            let mut bytes_received = 0u64;
            loop {
                match server_stream.read(&mut buffer).await {
                    Ok(0) => break,
                    Ok(n) => bytes_received += n as u64,
                    Err(_) => break,
                }
            }
            bytes_received
        })
    };

    let (_, received) = tokio::join!(send_handle, recv_handle);
    received.unwrap_or(0)
}

// Go-style benchmark: BenchmarkConnSmux equivalent
fn bench_conn_smux(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("go_style_smux");

    // Match Go benchmark settings
    group.sample_size(10);
    group.measurement_time(std::time::Duration::from_secs(5));
    group.throughput(Throughput::Bytes(GO_CHUNK_SIZE as u64));

    group.bench_function("BenchmarkConnSmux", |b| {
        b.iter(|| rt.block_on(async { black_box(go_style_benchmark_smux().await) }))
    });

    group.finish();
}

// Go-style benchmark: BenchmarkConnTCP equivalent
fn bench_conn_tcp(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("go_style_tcp");

    group.sample_size(10);
    group.measurement_time(std::time::Duration::from_secs(5));
    group.throughput(Throughput::Bytes(GO_CHUNK_SIZE as u64));

    group.bench_function("BenchmarkConnTCP", |b| {
        b.iter(|| rt.block_on(async { black_box(go_style_benchmark_tcp().await) }))
    });

    group.finish();
}

// Quick test for development
fn bench_quick_test(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("quick_test");

    group.sample_size(10);
    group.measurement_time(std::time::Duration::from_secs(2));
    group.warm_up_time(std::time::Duration::from_millis(500));
    group.throughput(Throughput::Bytes(QUICK_SMALL as u64));

    group.bench_function("1MB_simple", |b| {
        b.iter(|| rt.block_on(async { black_box(simple_throughput_test(QUICK_SMALL).await) }))
    });

    group.finish();
}

criterion_group!(benches, bench_conn_smux, bench_conn_tcp, bench_quick_test);
criterion_main!(benches);
