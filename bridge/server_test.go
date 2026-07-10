package bridge

import (
	"context"
	"crypto/sha1"
	"encoding/hex"
	"fmt"
	"net"
	"sync"
	"testing"
	"time"

	pb "dht/proto"

	"dht/node"
	"dht/testutil"

	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials/insecure"
)

const (
	bridgeTestFirstPort = 21200
	bridgeTestGRPCPort  = 21210 // must not collide with DHT node ports
	bridgeTestNodeCount = 1     // nodes 0 and 1 → ports 21200, 21201
)

// setupBridgeTest creates a small DHT network (2 nodes) and returns
// a gRPC client connected to node 0, plus cleanup functions.
func setupBridgeTest(t *testing.T) (pb.DhtServiceClient, []node.DhtNode, func()) {
	t.Helper()

	nodes := make([]node.DhtNode, bridgeTestNodeCount+1)

	testutil.Wg = new(sync.WaitGroup)
	for i := 0; i <= bridgeTestNodeCount; i++ {
		nodes[i] = node.NewNode(bridgeTestFirstPort + i)
		testutil.Wg.Add(1)
		go nodes[i].Run(testutil.Wg)
	}
	testutil.Wg.Wait()
	time.Sleep(200 * time.Millisecond)

	// Create network.
	nodes[0].Create()
	for i := 1; i <= bridgeTestNodeCount; i++ {
		addr := fmt.Sprintf("127.0.0.1:%d", bridgeTestFirstPort)
		if !nodes[i].Join(addr) {
			t.Fatalf("Node %d failed to join", i)
		}
		time.Sleep(200 * time.Millisecond)
	}
	time.Sleep(2 * time.Second)

	// Start gRPC server on node 0.
	lis, err := net.Listen("tcp", fmt.Sprintf(":%d", bridgeTestGRPCPort))
	if err != nil {
		t.Fatalf("Failed to listen on gRPC port: %v", err)
	}

	srv := grpc.NewServer()
	dhtSrv := NewDhtServer(nodes[0])
	pb.RegisterDhtServiceServer(srv, dhtSrv)

	go func() {
		_ = srv.Serve(lis)
	}()

	// Create gRPC client.
	conn, err := grpc.NewClient(
		fmt.Sprintf("127.0.0.1:%d", bridgeTestGRPCPort),
		grpc.WithTransportCredentials(insecure.NewCredentials()),
	)
	if err != nil {
		t.Fatalf("Failed to create gRPC client: %v", err)
	}

	client := pb.NewDhtServiceClient(conn)

	cleanup := func() {
		conn.Close()
		srv.GracefulStop()
		for i := 0; i <= bridgeTestNodeCount; i++ {
			nodes[i].Quit()
		}
	}

	return client, nodes, cleanup
}

func TestGRPCPutGetDelete(t *testing.T) {
	testutil.Yellow.Println("=== TestGRPCPutGetDelete ===")

	client, _, cleanup := setupBridgeTest(t)
	defer cleanup()

	ctx := context.Background()

	// ── Put ──
	putResp, err := client.Put(ctx, &pb.PutRequest{Key: "hello", Value: "world"})
	if err != nil {
		t.Fatalf("Put RPC failed: %v", err)
	}
	if !putResp.Success {
		t.Error("Put should succeed")
	}
	testutil.Green.Println("  ✓ Put OK")

	// ── Get (found) ──
	getResp, err := client.Get(ctx, &pb.GetRequest{Key: "hello"})
	if err != nil {
		t.Fatalf("Get RPC failed: %v", err)
	}
	if !getResp.Found || getResp.Value != "world" {
		t.Errorf("Get: expected (true, world), got (%v, %q)", getResp.Found, getResp.Value)
	}
	testutil.Green.Println("  ✓ Get (found) OK")

	// ── Get (not found) ──
	getResp, err = client.Get(ctx, &pb.GetRequest{Key: "nonexistent"})
	if err != nil {
		t.Fatalf("Get RPC failed: %v", err)
	}
	if getResp.Found {
		t.Error("Get on nonexistent key should return found=false")
	}
	testutil.Green.Println("  ✓ Get (not found) OK")

	// ── Delete ──
	delResp, err := client.Delete(ctx, &pb.DeleteRequest{Key: "hello"})
	if err != nil {
		t.Fatalf("Delete RPC failed: %v", err)
	}
	if !delResp.Success {
		t.Error("Delete should succeed")
	}

	// Verify deletion.
	getResp, _ = client.Get(ctx, &pb.GetRequest{Key: "hello"})
	if getResp.Found {
		t.Error("Key should be deleted")
	}
	testutil.Green.Println("  ✓ Delete OK")

	testutil.Yellow.Println("=== TestGRPCPutGetDelete PASSED ===")
}

func TestGRPCGetPeersAnnouncePeer(t *testing.T) {
	testutil.Yellow.Println("=== TestGRPCGetPeersAnnouncePeer ===")

	client, nodes, cleanup := setupBridgeTest(t)
	defer cleanup()

	ctx := context.Background()

	// Encode a test info_hash.
	rawHash := sha1Hash("test-torrent-file")
	infoHashHex := hex.EncodeToString(rawHash[:])

	// ── GetPeers (no peers yet) ──
	resp, err := client.GetPeers(ctx, &pb.GetPeersRequest{InfoHash: rawHash[:]})
	if err != nil {
		t.Fatalf("GetPeers RPC failed: %v", err)
	}
	if resp.Found || len(resp.Peers) > 0 {
		t.Error("GetPeers on unknown hash should return found=false, no peers")
	}
	testutil.Green.Println("  ✓ GetPeers (empty) OK")

	// ── AnnouncePeer ──
	annResp, err := client.AnnouncePeer(ctx, &pb.AnnouncePeerRequest{
		InfoHash: rawHash[:],
		Peer: &pb.Peer{
			Ip:   "10.0.0.1",
			Port: 6881,
		},
	})
	if err != nil {
		t.Fatalf("AnnouncePeer RPC failed: %v", err)
	}
	if !annResp.Success {
		t.Error("AnnouncePeer should succeed")
	}
	testutil.Green.Println("  ✓ AnnouncePeer OK")

	// Allow announcement to propagate.
	time.Sleep(500 * time.Millisecond)

	// ── GetPeers (should find the peer) ──
	resp, err = client.GetPeers(ctx, &pb.GetPeersRequest{InfoHash: rawHash[:]})
	if err != nil {
		t.Fatalf("GetPeers RPC failed: %v", err)
	}
	if !resp.Found {
		t.Error("GetPeers after announce: expected found=true")
	}
	if len(resp.Peers) == 0 {
		t.Fatal("GetPeers after announce: expected at least 1 peer")
	}
	if resp.Peers[0].Ip != "10.0.0.1" || resp.Peers[0].Port != 6881 {
		t.Errorf("Expected peer 10.0.0.1:6881, got %s:%d",
			resp.Peers[0].Ip, resp.Peers[0].Port)
	}
	testutil.Green.Println("  ✓ GetPeers (found) OK")

	// ── Announce a second peer ──
	_, err = client.AnnouncePeer(ctx, &pb.AnnouncePeerRequest{
		InfoHash: rawHash[:],
		Peer:     &pb.Peer{Ip: "10.0.0.2", Port: 6882},
	})
	if err != nil {
		t.Fatalf("AnnouncePeer (2nd) RPC failed: %v", err)
	}
	time.Sleep(500 * time.Millisecond)

	resp, err = client.GetPeers(ctx, &pb.GetPeersRequest{InfoHash: rawHash[:]})
	if err != nil {
		t.Fatalf("GetPeers RPC failed: %v", err)
	}
	if len(resp.Peers) < 2 {
		t.Errorf("Expected at least 2 peers, got %d", len(resp.Peers))
	}
	testutil.Green.Println("  ✓ Multiple peers OK")

	// ── Verify peer via another DHT node (node 1) ──
	peers, found := nodes[1].GetPeers(infoHashHex)
	if !found || len(peers) == 0 {
		t.Error("Other DHT node should find peers via DHT lookup")
	} else {
		testutil.Green.Println("  ✓ Cross-node lookup OK")
	}

	testutil.Yellow.Println("=== TestGRPCGetPeersAnnouncePeer PASSED ===")
}

// sha1Hash returns the SHA-1 hash of data as [20]byte.
func sha1Hash(data string) [20]byte {
	return sha1.Sum([]byte(data))
}
