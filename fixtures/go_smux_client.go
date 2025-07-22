package main

import (
	"fmt"
	"io"
	"log"
	"net"
	"os"
	"strconv"
	"time"

	"github.com/xtaci/smux"
)

func testSingleStream(session *smux.Session, testData string) error {
	fmt.Printf("📤 Opening stream and sending: %s\n", testData)

	stream, err := session.OpenStream()
	if err != nil {
		return fmt.Errorf("failed to open stream: %w", err)
	}
	defer stream.Close()

	// Set reasonable timeouts
	stream.SetWriteDeadline(time.Now().Add(10 * time.Second))
	stream.SetReadDeadline(time.Now().Add(10 * time.Second))

	// Send data
	_, err = stream.Write([]byte(testData))
	if err != nil {
		return fmt.Errorf("failed to write data: %w", err)
	}
	fmt.Printf("📡 Data sent successfully\n")

	// Give server time to process and respond
	time.Sleep(100 * time.Millisecond)

	// Read response using a simple approach
	response := make([]byte, len(testData))
	totalRead := 0

	for totalRead < len(testData) {
		n, err := stream.Read(response[totalRead:])
		if err != nil {
			if err == io.EOF && totalRead > 0 {
				fmt.Printf("📄 Got EOF after reading %d bytes\n", totalRead)
				break
			}
			return fmt.Errorf("failed to read response after %d bytes: %w", totalRead, err)
		}

		if n > 0 {
			totalRead += n
			fmt.Printf("📖 Read %d bytes, total: %d/%d\n", n, totalRead, len(testData))
		}

		// Small delay to allow more data to arrive
		if totalRead < len(testData) {
			time.Sleep(10 * time.Millisecond)
		}
	}

	responseStr := string(response[:totalRead])
	fmt.Printf("📥 Received echo: '%s'\n", responseStr)

	if responseStr != testData {
		return fmt.Errorf("response mismatch: expected '%s', got '%s'", testData, responseStr)
	}

	fmt.Printf("✅ Single stream test passed\n")
	return nil
}

func main() {
	if len(os.Args) < 2 {
		fmt.Printf("Usage: %s <port> [num_streams]\n", os.Args[0])
		os.Exit(1)
	}

	port, err := strconv.Atoi(os.Args[1])
	if err != nil {
		log.Fatalf("Invalid port: %v", err)
	}

	numStreams := 1
	if len(os.Args) > 2 {
		numStreams, err = strconv.Atoi(os.Args[2])
		if err != nil {
			log.Fatalf("Invalid number of streams: %v", err)
		}
	}

	fmt.Printf("🔗 Connecting to Rust smux server on port %d\n", port)

	// Connect to Rust server
	conn, err := net.Dial("tcp", fmt.Sprintf("127.0.0.1:%d", port))
	if err != nil {
		log.Fatalf("Failed to connect: %v", err)
	}
	defer conn.Close()

	fmt.Printf("✅ TCP connection established\n")

	// Create smux client session with version 1 config for compatibility
	config := smux.DefaultConfig()
	config.Version = 1
	config.MaxReceiveBuffer = 4194304
	config.MaxStreamBuffer = 65536
	session, err := smux.Client(conn, config)
	if err != nil {
		log.Fatalf("Failed to create smux client: %v", err)
	}
	defer session.Close()

	fmt.Printf("✅ Smux client session created\n")

	// Wait a moment for session to be fully established
	time.Sleep(100 * time.Millisecond)

	// Test streams sequentially with delays
	for i := 0; i < numStreams; i++ {
		testData := fmt.Sprintf("stream_%d_from_go_client", i)
		fmt.Printf("🧪 Testing stream %d/%d\n", i+1, numStreams)

		if err := testSingleStream(session, testData); err != nil {
			log.Fatalf("Stream %d test failed: %v", i, err)
		}

		// Delay between streams
		if i < numStreams-1 {
			time.Sleep(200 * time.Millisecond)
		}
	}

	fmt.Printf("🎉 All %d stream tests completed successfully!\n", numStreams)
}