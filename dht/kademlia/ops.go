package kademlia

import (
	"encoding/hex"
	"fmt"
	"sync"

	"github.com/sirupsen/logrus"
)

// Put stores a key-value pair in the DHT
// Store the pair in the K-closest nodes from the target, and also locally
// so that the writing node always reflects the latest value it wrote.
func (node *KademliaNode) Put(key string, value string) bool {
	logrus.Infof("[%s] Putting key: %s, value: %s", node.Addr, key, value)

	keyID := hash(key)

	// Store locally FIRST so that a subsequent Get on this node
	// returns the latest value, even before remote stores complete.
	node.dataLock.Lock()
	node.data[key] = value
	node.dataLock.Unlock()

	success := 1 // count the local store as one success
	var mu sync.Mutex
	var wg sync.WaitGroup

	closest := node.findNode(keyID)

	for _, c := range closest {
		wg.Add(1)
		go func(contact Contact) {
			defer wg.Done()
			args := StoreArgs{
				Key:   key,
				Value: value,
			}
			err := node.RemoteCall(contact.Addr, "KademliaNode.Store", &args, &struct{}{})
			if err != nil {
				logrus.Errorf("[%s] Failed to store data on %s: %v", node.Addr, contact.Addr, err)
			} else {
				logrus.Infof("[%s] Successfully stored data on %s", node.Addr, contact.Addr)
				mu.Lock()
				success++
				mu.Unlock()
			}
		}(c)
	}

	wg.Wait()
	return success > 0
}

// Get retrieves a value from the DHT using the provided key
func (node *KademliaNode) Get(key string) (bool, string) {
	logrus.Infof("[%s] Getting key: %s", node.Addr, key)

	keyID := hash(key)

	node.dataLock.RLock()
	if val, ok := node.data[key]; ok {
		node.dataLock.RUnlock()
		return true, val
	}
	node.dataLock.RUnlock()

	shortlist := node.findClosestContacts(keyID, K)
	if len(shortlist) == 0 {
		return false, ""
	}

	queried := make(map[string]bool)

	for i := 0; i < 32; i++ {
		candidates := node.getAlphaClosestContacts(shortlist, keyID, queried)
		if len(candidates) == 0 {
			break
		}

		found := make(chan string, len(candidates))
		foundContacts := make(chan []Contact, len(candidates))
		var failedMu sync.Mutex
		failedAddrs := make(map[string]bool)
		var wg sync.WaitGroup

		for _, c := range candidates {
			queried[c.Addr] = true
			wg.Add(1)
			go func(contact Contact) {
				defer wg.Done()
				var reply FindValueReply
				err := node.RemoteCall(contact.Addr, "KademliaNode.FindValue", key, &reply)
				if err != nil {
					logrus.Errorf("[%s] Failed to find value on %s: %v", node.Addr, contact.Addr, err)
					failedMu.Lock()
					failedAddrs[contact.Addr] = true
					failedMu.Unlock()
					return
				}
				if reply.Found {
					found <- reply.Value
				} else {
					foundContacts <- reply.Contacts
				}
			}(c)
		}

		wg.Wait()
		close(found)
		close(foundContacts)

		// Remove dead contacts from shortlist so they don't cause
		// isLimitReached to stop the search prematurely.
		if len(failedAddrs) > 0 {
			alive := make([]Contact, 0, len(shortlist))
			for _, c := range shortlist {
				if !failedAddrs[c.Addr] {
					alive = append(alive, c)
				}
			}
			shortlist = alive
		}

		// Drain found values — a closed empty channel yields zero values,
		// so we must drain before deciding whether a value was found.
		foundValues := []string{}
		for v := range found {
			foundValues = append(foundValues, v)
		}
		if len(foundValues) > 0 {
			go node.cacheValue(key, foundValues[0], keyID)
			return true, foundValues[0]
		}

		for contacts := range foundContacts {
			for _, c := range contacts {
				if c.Addr != node.Addr {
					node.mergeShortlist(&shortlist, c, keyID, K)
				}
			}
		}

		if node.isLimitReached(shortlist, queried, keyID, K) {
			break
		}
	}

	return false, ""
}

// Cache values in the K closest nodes to the keyID, including locally,
// so that subsequent Gets on this node are served from local cache.
func (node *KademliaNode) cacheValue(key, value string, keyID [IDLength]byte) {
	// Store locally first.
	node.dataLock.Lock()
	node.data[key] = value
	node.dataLock.Unlock()

	closest := node.findNode(keyID)

	for _, c := range closest {
		if c.Addr != node.Addr {
			args := StoreArgs{
				Key:   key,
				Value: value,
			}
			err := node.RemoteCall(c.Addr, "KademliaNode.Store", &args, &struct{}{})
			if err != nil {
				logrus.Errorf("[%s] Failed to cache value on %s: %v", node.Addr, c.Addr, err)
			} else {
				logrus.Infof("[%s] Successfully cached value on %s", node.Addr, c.Addr)
			}
		}
	}
}

// Delete removes a key-value pair from the DHT.
// It deletes the key from the local node (if present) and from the K closest
// remote nodes. Returns true if at least one replica confirmed deletion.
func (node *KademliaNode) Delete(key string) bool {
	logrus.Infof("[%s] Deleting key: %s", node.Addr, key)

	keyID := hash(key)

	localDeleted := false
	node.dataLock.Lock()
	if _, exists := node.data[key]; exists {
		delete(node.data, key)
		localDeleted = true
	}
	node.dataLock.Unlock()

	closest := node.findNode(keyID)

	remoteDeleted := false
	var mu sync.Mutex
	var wg sync.WaitGroup

	for _, c := range closest {
		if c.Addr == node.Addr {
			continue // already handled locally
		}
		wg.Add(1)
		go func(contact Contact) {
			defer wg.Done()
			var deleted bool
			err := node.RemoteCall(contact.Addr,
				"KademliaNode.DeleteData", key, &deleted)
			if err != nil {
				logrus.Errorf("[%s] Failed to delete key on %s: %v",
					node.Addr, contact.Addr, err)
				return
			}
			if deleted {
				logrus.Infof("[%s] Successfully deleted key on %s",
					node.Addr, contact.Addr)
				mu.Lock()
				remoteDeleted = true
				mu.Unlock()
			}
		}(c)
	}
	wg.Wait()

	return localDeleted || remoteDeleted
}

// decodeInfoHash converts a hex-encoded info_hash string (40 characters)
// into a [IDLength]byte. Returns false if the string is malformed.
func decodeInfoHash(hexStr string) ([IDLength]byte, bool) {
	var infoHash [IDLength]byte
	decoded, err := hex.DecodeString(hexStr)
	if err != nil || len(decoded) != IDLength {
		return infoHash, false
	}
	copy(infoHash[:], decoded)
	return infoHash, true
}

// GetPeers finds peers for a given info_hash (hex-encoded, 40 characters).
// It first checks the local peer store, then performs an iterative Kademlia
// lookup. Returns the aggregated peer list and whether any peers were found.
func (node *KademliaNode) GetPeers(infoHashHex string) ([]Peer, bool) {
	infoHash, ok := decodeInfoHash(infoHashHex)
	if !ok {
		logrus.Errorf("[%s] GetPeers: invalid info_hash %q", node.Addr, infoHashHex)
		return nil, false
	}

	logrus.Infof("[%s] GetPeers for info_hash %x", node.Addr, infoHash[:8])

	// Collect all peers (local + remote), deduplicated by "ip:port".
	peerSet := make(map[string]Peer)

	// Seed with local peer store entries first.
	node.peerStoreLock.RLock()
	if peers, exists := node.peerStore[infoHash]; exists {
		for _, p := range peers {
			key := fmt.Sprintf("%s:%d", p.IP, p.Port)
			if _, ok := peerSet[key]; !ok {
				peerSet[key] = p
			}
		}
	}
	node.peerStoreLock.RUnlock()

	// Always perform iterative lookup to discover peers on other nodes.
	shortlist := node.findClosestContacts(infoHash, K)
	if len(shortlist) == 0 {
		if len(peerSet) > 0 {
			result := make([]Peer, 0, len(peerSet))
			for _, p := range peerSet {
				result = append(result, p)
			}
			logrus.Infof("[%s] GetPeers found %d peers (local only) for info_hash %x",
				node.Addr, len(result), infoHash[:8])
			return result, true
		}
		return nil, false
	}

	queried := make(map[string]bool)

	for i := 0; i < 32; i++ {
		candidates := node.getAlphaClosestContacts(shortlist, infoHash, queried)
		if len(candidates) == 0 {
			break
		}

		foundPeers := make(chan []Peer, len(candidates))
		foundContacts := make(chan []Contact, len(candidates))
		var failedMu sync.Mutex
		failedAddrs := make(map[string]bool)
		var wg sync.WaitGroup

		for _, c := range candidates {
			queried[c.Addr] = true
			wg.Add(1)
			go func(contact Contact) {
				defer wg.Done()
				args := GetPeersArgs{
					InfoHash:   infoHash,
					CallerAddr: node.Addr,
				}
				var reply GetPeersReply
				err := node.RemoteCall(contact.Addr, "KademliaNode.GetPeersRPC", &args, &reply)
				if err != nil {
					logrus.Errorf("[%s] GetPeers RPC failed on %s: %v",
						node.Addr, contact.Addr, err)
					failedMu.Lock()
					failedAddrs[contact.Addr] = true
					failedMu.Unlock()
					return
				}
				if len(reply.Peers) > 0 {
					foundPeers <- reply.Peers
				}
				if len(reply.Nodes) > 0 {
					foundContacts <- reply.Nodes
				}
			}(c)
		}

		wg.Wait()
		close(foundPeers)
		close(foundContacts)

		// Remove dead contacts from shortlist.
		if len(failedAddrs) > 0 {
			alive := make([]Contact, 0, len(shortlist))
			for _, c := range shortlist {
				if !failedAddrs[c.Addr] {
					alive = append(alive, c)
				}
			}
			shortlist = alive
		}

		// Collect peers from this round (deduplicate by "ip:port").
		for peers := range foundPeers {
			for _, p := range peers {
				key := fmt.Sprintf("%s:%d", p.IP, p.Port)
				if _, exists := peerSet[key]; !exists {
					peerSet[key] = p
				}
			}
		}

		// Merge newly discovered contacts into the shortlist.
		for contacts := range foundContacts {
			for _, c := range contacts {
				if c.Addr != node.Addr {
					node.mergeShortlist(&shortlist, c, infoHash, K)
				}
			}
		}

		if node.isLimitReached(shortlist, queried, infoHash, K) {
			break
		}
	}

	if len(peerSet) > 0 {
		result := make([]Peer, 0, len(peerSet))
		for _, p := range peerSet {
			result = append(result, p)
		}
		logrus.Infof("[%s] GetPeers found %d peers for info_hash %x",
			node.Addr, len(result), infoHash[:8])
		return result, true
	}

	return nil, false
}

// AnnouncePeer registers a peer for the given info_hash (hex-encoded).
// It first stores the peer locally, then announces to the K closest nodes.
// Each announcement requires a per-node token obtained via GetPeers.
// Returns true if at least one remote node accepted the announcement.
func (node *KademliaNode) AnnouncePeer(infoHashHex string, peer Peer) bool {
	infoHash, ok := decodeInfoHash(infoHashHex)
	if !ok {
		logrus.Errorf("[%s] AnnouncePeer: invalid info_hash %q", node.Addr, infoHashHex)
		return false
	}

	logrus.Infof("[%s] AnnouncePeer %s:%d for info_hash %x",
		node.Addr, peer.IP, peer.Port, infoHash[:8])

	// Store locally (deduplicated).
	node.peerStoreLock.Lock()
	peers := node.peerStore[infoHash]
	for _, p := range peers {
		if p.IP == peer.IP && p.Port == peer.Port {
			node.peerStoreLock.Unlock()
			return true // already registered
		}
	}
	node.peerStore[infoHash] = append(peers, peer)
	node.peerStoreLock.Unlock()

	// Find the K closest nodes via iterative lookup.
	closest := node.findNode(infoHash)

	// Phase 1: obtain a per-node token from each close node.
	type nodeToken struct {
		addr  string
		token string
	}
	var tokens []nodeToken
	var mu sync.Mutex
	var wg sync.WaitGroup

	for _, c := range closest {
		if c.Addr == node.Addr {
			continue
		}
		wg.Add(1)
		go func(contact Contact) {
			defer wg.Done()
			args := GetPeersArgs{
				InfoHash:   infoHash,
				CallerAddr: node.Addr,
			}
			var reply GetPeersReply
			err := node.RemoteCall(contact.Addr, "KademliaNode.GetPeersRPC", &args, &reply)
			if err != nil {
				logrus.Errorf("[%s] AnnouncePeer: failed to get token from %s: %v",
					node.Addr, contact.Addr, err)
				return
			}
			mu.Lock()
			tokens = append(tokens, nodeToken{addr: contact.Addr, token: reply.Token})
			mu.Unlock()
		}(c)
	}
	wg.Wait()

	if len(tokens) == 0 {
		// No remote nodes reached, but we stored locally → partial success.
		return true
	}

	// Phase 2: announce to each node using its own token.
	success := 0
	for _, nt := range tokens {
		announceArgs := AnnouncePeerArgs{
			InfoHash:   infoHash,
			Token:      nt.token,
			Peer:       peer,
			CallerAddr: node.Addr,
		}
		err := node.RemoteCall(nt.addr, "KademliaNode.AnnouncePeerRPC", &announceArgs, &struct{}{})
		if err != nil {
			logrus.Errorf("[%s] AnnouncePeer: announce to %s failed: %v",
				node.Addr, nt.addr, err)
		} else {
			success++
		}
	}

	logrus.Infof("[%s] AnnouncePeer: announced to %d/%d nodes",
		node.Addr, success, len(tokens))
	return success > 0
}
