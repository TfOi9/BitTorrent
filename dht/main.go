package main

import (
	"bufio"
	"flag"
	"fmt"
	"io"
	"net"
	"os"
	"os/signal"
	"strings"
	"sync"
	"time"

	"dht/bridge"
	"dht/node"
	pb "dht/proto"

	"google.golang.org/grpc"
	"google.golang.org/grpc/reflection"
)

func execCmd(n node.DhtNode, line string, w io.Writer) {
	parts := strings.Fields(line)
	if len(parts) == 0 {
		return
	}
	switch parts[0] {
	case "put":
		if len(parts) < 3 {
			fmt.Fprintln(w, "usage: put <key> <value>")
			return
		}
		if n.Put(parts[1], parts[2]) {
			fmt.Fprintln(w, "true")
		} else {
			fmt.Fprintln(w, "false")
		}
	case "get":
		if len(parts) < 2 {
			fmt.Fprintln(w, "usage: get <key>")
			return
		}
		ok, value := n.Get(parts[1])
		if ok {
			fmt.Fprintln(w, value)
		} else {
			fmt.Fprintln(w, "false")
		}
	case "delete":
		if len(parts) < 2 {
			fmt.Fprintln(w, "usage: delete <key>")
			return
		}
		if n.Delete(parts[1]) {
			fmt.Fprintln(w, "true")
		} else {
			fmt.Fprintln(w, "false")
		}
	case "quit":
		n.Quit()
		os.Exit(0)
	default:
		fmt.Fprintf(w, "unknown command: %s\n", parts[0])
	}
}

func main() {
	port := flag.Int("port", 20000, "port to listen on")
	join := flag.String("join", "", "address of existing node to join")
	addr := flag.String("addr", "127.0.0.1", "address to advertise to other nodes")
	cmdPort := flag.Int("cmd-port", 0, "port for CLI commands over TCP (0 to disable)")
	grpcPort := flag.Int("grpc-port", 0, "port for gRPC server (0 to disable)")
	flag.Parse()

	node.SetLocalAddress(*addr)
	n := node.NewNode(*port)

	var wg sync.WaitGroup
	wg.Add(1)
	go n.Run(&wg)
	wg.Wait()

	if *join != "" {
		for i := 0; i < 30; i++ {
			if n.Join(*join) {
				break
			}
			time.Sleep(time.Second)
		}
	} else {
		n.Create()
	}

	if *cmdPort > 0 {
		go startCmdServer(n, *cmdPort)
	}

	// Start gRPC server if a port is specified.
	var grpcServer *grpc.Server
	if *grpcPort > 0 {
		grpcServer = startGRPCServer(n, *grpcPort)
	}

	sig := make(chan os.Signal, 1)
	signal.Notify(sig, os.Interrupt)

	scanner := bufio.NewScanner(os.Stdin)
	cmdCh := make(chan string)

	go func() {
		for scanner.Scan() {
			cmdCh <- scanner.Text()
		}
		close(cmdCh)
	}()

	for {
		select {
		case <-sig:
			fmt.Println("received interrupt")
			if grpcServer != nil {
				grpcServer.GracefulStop()
			}
			n.Quit()
			return
		case line, ok := <-cmdCh:
			if !ok {
				// stdin closed (e.g., running as daemon).
				// If gRPC is active, keep serving; otherwise exit.
				if grpcServer != nil {
					fmt.Println("stdin closed, running in headless mode (Ctrl-C to stop)")
					// Block until signal.
					<-sig
					fmt.Println("received interrupt")
					grpcServer.GracefulStop()
					n.Quit()
					return
				}
				n.Quit()
				return
			}
			execCmd(n, line, os.Stdout)
		}
	}
}

func startCmdServer(n node.DhtNode, port int) {
	listener, err := net.Listen("tcp", fmt.Sprintf(":%d", port))
	if err != nil {
		fmt.Fprintf(os.Stderr, "cmd server listen error: %v\n", err)
		return
	}
	for {
		conn, err := listener.Accept()
		if err != nil {
			continue
		}
		go func(c net.Conn) {
			defer c.Close()
			scanner := bufio.NewScanner(c)
			if scanner.Scan() {
				execCmd(n, scanner.Text(), c)
			}
		}(conn)
	}
}

// startGRPCServer creates and starts a gRPC server on the given port,
// serving the DhtService API. It returns the server handle for graceful shutdown.
func startGRPCServer(n node.DhtNode, port int) *grpc.Server {
	lis, err := net.Listen("tcp", fmt.Sprintf(":%d", port))
	if err != nil {
		fmt.Fprintf(os.Stderr, "gRPC server listen error: %v\n", err)
		return nil
	}

	srv := grpc.NewServer()
	dhtSrv := bridge.NewDhtServer(n)
	pb.RegisterDhtServiceServer(srv, dhtSrv)

	// Enable server reflection for debugging with grpcurl.
	reflection.Register(srv)

	fmt.Printf("gRPC server listening on :%d\n", port)
	go func() {
		if err := srv.Serve(lis); err != nil {
			fmt.Fprintf(os.Stderr, "gRPC server error: %v\n", err)
		}
	}()
	return srv
}
