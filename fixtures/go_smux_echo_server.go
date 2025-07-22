package main

import (
    "fmt"
    "io"
    "net"
    "os"
    "time"

    "github.com/xtaci/smux"
)

func handleStream(stream *smux.Stream) {
    defer stream.Close()

    // Set read timeout to avoid hanging
    stream.SetReadDeadline(time.Now().Add(10 * time.Second))

    // Read data from the stream
    data, err := io.ReadAll(stream)
    if err != nil {
        return
    }

    stream.SetWriteDeadline(time.Now().Add(5 * time.Second))
    _, err = stream.Write(data)
    if err != nil {
        return
    }
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

    for {
        conn, err := listener.Accept()
        if err != nil {
            fmt.Printf("Accept error: %v\n", err)
            continue
        }

        go handleConnection(conn)
    }
}

func handleConnection(conn net.Conn) {
    defer conn.Close()

    config := smux.DefaultConfig()
    config.Version = 1
    session, err := smux.Server(conn, config)
    if err != nil {
        fmt.Printf("Smux server error: %v\n", err)
        return
    }
    defer session.Close()

    for {
        stream, err := session.AcceptStream()
        if err != nil {
            fmt.Printf("Accept stream error: %v\n", err)
            break
        }
        go handleStream(stream)
    }

}