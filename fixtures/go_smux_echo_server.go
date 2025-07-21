package main

import (
    "fmt"
    "io"
    "net"
    "os"
    "sync"
    "time"

    "github.com/xtaci/smux"
)

func handleStream(stream *smux.Stream, streamNum int) {
    defer stream.Close()

    fmt.Printf("Handling stream %d\n", streamNum)

    // Set read timeout to avoid hanging
    stream.SetReadDeadline(time.Now().Add(10 * time.Second))

    // Read data from the stream
    data, err := io.ReadAll(stream)
    if err != nil {
        fmt.Printf("Stream %d read error: %v\n", streamNum, err)
        return
    }

    fmt.Printf("Stream %d received: %s\n", streamNum, string(data))

    // Echo back the data
    stream.SetWriteDeadline(time.Now().Add(5 * time.Second))
    _, err = stream.Write(data)
    if err != nil {
        fmt.Printf("Stream %d write error: %v\n", streamNum, err)
        return
    }

    fmt.Printf("Stream %d echoed back: %s\n", streamNum, string(data))
}

func main() {
    port := "54321" // default port
    if len(os.Args) > 1 {
        port = os.Args[1]
    }

    listener, err := net.Listen("tcp", "127.0.0.1:"+port)
    if err != nil {
        fmt.Printf("Listen error: %v\n", err)
        return
    }
    defer listener.Close()

    fmt.Printf("Go smux echo server listening on port %s\n", port)

    // Accept multiple TCP connections
    for {
        conn, err := listener.Accept()
        if err != nil {
            fmt.Printf("Accept error: %v\n", err)
            continue
        }

        fmt.Printf("TCP connection accepted\n")

        // Handle each connection in a goroutine
        go handleConnection(conn)
    }
}

func handleConnection(conn net.Conn) {
    defer conn.Close()

    // Create smux server session with version 1 config
    config := smux.DefaultConfig()
    config.Version = 1
    session, err := smux.Server(conn, config)
    if err != nil {
        fmt.Printf("Smux server error: %v\n", err)
        return
    }
    defer session.Close()

    fmt.Printf("Smux server session created\n")

    var wg sync.WaitGroup
    streamCount := 0

    // Accept streams in a loop with better error handling
    for {
        stream, err := session.AcceptStream()
        if err != nil {
            fmt.Printf("Accept stream error: %v\n", err)
            // Break only on EOF or connection closed, not on temporary errors
            if err.Error() == "EOF" || err.Error() == "io: read/write on closed pipe" {
                break
            }
            continue
        }

        wg.Add(1)
        go func(s *smux.Stream, num int) {
            defer wg.Done()
            handleStream(s, num)
        }(stream, streamCount)

        streamCount++
        fmt.Printf("Accepted stream %d\n", streamCount)

        // Stop after reasonable number of streams for testing
        if streamCount >= 25 {
            break
        }
    }

    // Wait for all streams to complete with reasonable timeout
    done := make(chan struct{})
    go func() {
        wg.Wait()
        close(done)
    }()

    select {
    case <-done:
        fmt.Printf("All %d streams processed for this connection\n", streamCount)
    case <-time.After(30 * time.Second):
        fmt.Printf("Timeout after 30 seconds, processed %d streams for this connection\n", streamCount)
    }
}