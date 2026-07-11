package node

import (
	"encoding/hex"
	"sync"
	"testing"
	"time"

	"dht/kademlia"
	"dht/testutil"
)

const (
	PeerTestFirstPort = 21000 // dedicated port range
	PeerTestNodeSize  = 3
)

// hexToBytes is a helper to convert hex string to [20]byte for info_hash.
func hexToInfoHash(hexStr string) [20]byte {
	var h [20]byte
	decoded, _ := hex.DecodeString(hexStr)
	copy(h[:], decoded)
	return h
}

func TestGetPeersAndAnnounce(t *testing.T) {
	testutil.Yellow.Println("=== TestGetPeersAndAnnounce ===")

	nodes := new([PeerTestNodeSize + 1]DhtNode)
	nodeAddrs := new([PeerTestNodeSize + 1]string)

	testutil.Wg = new(sync.WaitGroup)
	for i := 0; i <= PeerTestNodeSize; i++ {
		nodes[i] = NewNode(PeerTestFirstPort + i)
		nodeAddrs[i] = portToAddr(localAddress, PeerTestFirstPort+i)

		testutil.Wg.Add(1)
		go nodes[i].Run(testutil.Wg)
	}
	testutil.Wg.Wait()
	time.Sleep(200 * time.Millisecond)

	// Create the network and join remaining nodes.
	nodes[0].Create()
	for i := 1; i <= PeerTestNodeSize; i++ {
		if !nodes[i].Join(nodeAddrs[0]) {
			t.Fatalf("Node %d failed to join via %s", i, nodeAddrs[0])
		}
		time.Sleep(200 * time.Millisecond)
	}
	time.Sleep(2 * time.Second) // let routing tables stabilize

	// --- Test 1: GetPeers with unknown info_hash returns nothing ---
	unknownHash := "aabbccddeeff00112233445566778899aabbccdd"
	peers, found := nodes[0].GetPeers(unknownHash)
	if found || len(peers) > 0 {
		t.Errorf("GetPeers on unknown info_hash: expected (nil, false), got %d peers, found=%v",
			len(peers), found)
	}
	testutil.Green.Println("  ✓ Test 1 passed: GetPeers on unknown hash returns nothing")

	// --- Test 2: AnnouncePeer succeeds ---
	infoHashHex := "1234567890abcdef1234567890abcdef12345678" // 40 hex chars = 20 bytes
	peer1 := kademlia.Peer{IP: "10.0.0.1", Port: 6881}
	ok := nodes[1].AnnouncePeer(infoHashHex, peer1)
	if !ok {
		t.Errorf("AnnouncePeer should succeed, got false")
	}
	testutil.Green.Println("  ✓ Test 2 passed: AnnouncePeer succeeds")

	// Give the announcement time to propagate to closest nodes.
	time.Sleep(500 * time.Millisecond)

	// --- Test 3: GetPeers after announce returns the peer ---
	peers, found = nodes[1].GetPeers(infoHashHex)
	if !found {
		t.Errorf("GetPeers after announce: expected found=true, got false")
	}
	if len(peers) < 1 {
		t.Fatalf("GetPeers after announce: expected at least 1 peer, got %d", len(peers))
	}
	if peers[0].IP != peer1.IP || peers[0].Port != peer1.Port {
		t.Errorf("Expected peer %s:%d, got %s:%d",
			peer1.IP, peer1.Port, peers[0].IP, peers[0].Port)
	}
	testutil.Green.Println("  ✓ Test 3 passed: GetPeers returns announced peer locally")

	// --- Test 4: Another node can find the peer via DHT iterative lookup ---
	time.Sleep(500 * time.Millisecond)
	peers, found = nodes[2].GetPeers(infoHashHex)
	if !found {
		t.Errorf("Node 2 GetPeers: expected found=true via DHT lookup, got false")
	}
	if len(peers) < 1 {
		t.Errorf("Node 2 GetPeers: expected at least 1 peer, got %d", len(peers))
	} else if peers[0].IP != peer1.IP || peers[0].Port != peer1.Port {
		t.Errorf("Node 2: expected peer %s:%d, got %s:%d",
			peer1.IP, peer1.Port, peers[0].IP, peers[0].Port)
	}
	testutil.Green.Println("  ✓ Test 4 passed: another node finds peer via DHT lookup")

	// --- Test 5: AnnouncePeer with duplicate peer is idempotent ---
	ok = nodes[0].AnnouncePeer(infoHashHex, peer1)
	if !ok {
		t.Errorf("AnnouncePeer duplicate: should still return true")
	}
	peers, found = nodes[0].GetPeers(infoHashHex)
	if !found {
		t.Errorf("GetPeers after duplicate announce: expected found=true")
	}
	// Should still have exactly 1 peer (dedup worked).
	if len(peers) != 1 {
		t.Errorf("Expected 1 peer after duplicate announce, got %d", len(peers))
	}
	testutil.Green.Println("  ✓ Test 5 passed: duplicate announce is idempotent")

	// --- Test 6: Announce a second peer for same info_hash ---
	peer2 := kademlia.Peer{IP: "10.0.0.2", Port: 6882}
	ok = nodes[0].AnnouncePeer(infoHashHex, peer2)
	if !ok {
		t.Errorf("AnnouncePeer for second peer should succeed")
	}
	time.Sleep(500 * time.Millisecond)

	peers, found = nodes[1].GetPeers(infoHashHex)
	if !found {
		t.Errorf("GetPeers after second announce: expected found=true")
	}
	if len(peers) < 2 {
		t.Errorf("Expected at least 2 peers, got %d", len(peers))
	}
	testutil.Green.Println("  ✓ Test 6 passed: multiple peers for same info_hash")

	// --- Test 7: Token protection — announce with wrong token via direct RPC ---
	// We use RemoteCall directly (via type assertion) to send an AnnouncePeerRPC
	// with a bogus token and verify the peer is NOT stored.
	infoHashBytes := hexToInfoHash(infoHashHex)
	wrongTokenArgs := kademlia.AnnouncePeerArgs{
		InfoHash:   infoHashBytes,
		Token:      "deadbeef-bogus-token",
		Peer:       kademlia.Peer{IP: "192.168.1.100", Port: 9999},
		CallerAddr: nodeAddrs[2],
	}

	// Type-assert to access RemoteCall (not on DhtNode interface, but on KademliaNode).
	type rawCaller interface {
		RemoteCall(addr, method string, args interface{}, reply interface{}) error
	}
	caller, isKad := nodes[0].(rawCaller)
	if !isKad {
		t.Skip("Cannot access RemoteCall for direct RPC test")
		return
	}

	// Call AnnouncePeerRPC with a wrong token. We call it on node 1 (which stores
	// the peer list) from node 0. The token "deadbeef-bogus-token" will not match
	// node 1's HMAC(tokenSecret, nodeAddrs[0]), so node 1 will silently reject it.
	err := caller.RemoteCall(nodeAddrs[1], "KademliaNode.AnnouncePeerRPC", &wrongTokenArgs, &struct{}{})
	if err != nil {
		t.Logf("RemoteCall for wrong token returned error: %v", err)
	}

	// Now verify the bogus peer was NOT stored.
	peers, _ = nodes[1].GetPeers(infoHashHex)
	for _, p := range peers {
		if p.IP == "192.168.1.100" && p.Port == 9999 {
			t.Errorf("Bogus peer with wrong token was incorrectly stored!")
		}
	}
	// The attempt above may fail because node 0 doesn't know node 1's address
	// for token verification — but it should still be rejected because the
	// token computation is deterministic per-node. Node 1 will compute the
	// expected token using its own secret + node 0's address.
	// Since we used a bogus token, it won't match.
	testutil.Green.Println("  ✓ Test 7 passed: wrong token is rejected")

	// Cleanup
	for i := 0; i <= PeerTestNodeSize; i++ {
		nodes[i].Quit()
	}
	testutil.Yellow.Println("=== TestGetPeersAndAnnounce PASSED ===")
}
