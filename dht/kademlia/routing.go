package kademlia

import (
	"crypto/rand"
	"crypto/sha1"
	"math/big"
	"sort"
	"sync"
	"time"
)

// tuple of (IP, port, ID), corresponding to the Kademlia paper
type Contact struct {
	NodeID   [IDLength]byte
	Addr     string // IP:port
	LastSeen time.Time
}

// K-bucket
type KBucket struct {
	contacts []Contact
	mu       sync.Mutex
}

// K-Bucket methods

// Checks if a contact is already in the k-bucket
func (bucket *KBucket) contains(contact Contact) bool {
	bucket.mu.Lock()
	defer bucket.mu.Unlock()

	for _, c := range bucket.contacts {
		if c.NodeID == contact.NodeID {
			return true
		}
	}
	return false
}

// Checks if a k-bucket is full
func (bucket *KBucket) isFull() bool {
	bucket.mu.Lock()
	defer bucket.mu.Unlock()

	return len(bucket.contacts) >= K
}

// Update an existing contact's LastSeen time, moving it to the tail
func (bucket *KBucket) updateContact(contact Contact) {
	bucket.mu.Lock()
	defer bucket.mu.Unlock()

	for i, c := range bucket.contacts {
		if c.NodeID == contact.NodeID {
			contact.LastSeen = time.Now()
			bucket.contacts = append(bucket.contacts[:i], bucket.contacts[i+1:]...)
			bucket.contacts = append(bucket.contacts, contact)
			return
		}
	}
}

// Use SHA-1 to hash the key and return a 20-byte array
func hash(key string) [IDLength]byte {
	return sha1.Sum([]byte(key))
}

// Calculates the XOR-distance between two node IDs
func xorDistance(id1, id2 [IDLength]byte) *big.Int {
	var xor [IDLength]byte
	for i := 0; i < IDLength; i++ {
		xor[i] = id1[i] ^ id2[i]
	}
	return new(big.Int).SetBytes(xor[:])
}

// Returns the index of the k-bucket for a given node ID
// i.e. floor(log2(xor-distance))
func bucketIndex(id1, id2 [IDLength]byte) int {
	dist := xorDistance(id1, id2)
	if dist.Sign() == 0 {
		return KBucketCount - 1
	}
	return int(dist.BitLen() - 1)
}

// Returns the first count-closest contacts to the target from all buckets
func (node *KademliaNode) findClosestContacts(target [IDLength]byte, count int) []Contact {
	node.bucketsLock.RLock()
	defer node.bucketsLock.RUnlock()

	allContacts := []Contact{}
	for _, bucket := range node.buckets {
		if bucket != nil {
			bucket.mu.Lock()
			allContacts = append(allContacts, bucket.contacts...)
			bucket.mu.Unlock()
		}
	}

	sort.Slice(allContacts, func(i, j int) bool {
		return xorDistance(allContacts[i].NodeID, target).Cmp(
			xorDistance(allContacts[j].NodeID, target)) < 0
	})

	if count > len(allContacts) {
		return allContacts
	}
	return allContacts[:count]
}

// Insert a new contact into the appropriate k-bucket
func (node *KademliaNode) insertContact(contact Contact) {
	if contact.NodeID == node.NodeID {
		return
	}

	idx := bucketIndex(node.NodeID, contact.NodeID)
	if idx < 0 || idx >= KBucketCount {
		return
	}

	node.bucketsLock.RLock()
	bucket := node.buckets[idx]
	node.bucketsLock.RUnlock()

	if bucket.contains(contact) {
		bucket.updateContact(contact)
		return
	}

	if !bucket.isFull() {
		bucket.mu.Lock()
		bucket.contacts = append(bucket.contacts, contact)
		bucket.mu.Unlock()
		return
	}

	bucket.mu.Lock()
	head := bucket.contacts[0]
	bucket.mu.Unlock()

	// Use rawCall to avoid cascading Introduce → insertContact → Ping chains.
	err := node.rawCall(head.Addr, "KademliaNode.Ping", "", &struct{}{})
	if err != nil {
		bucket.mu.Lock()
		bucket.contacts = bucket.contacts[1:]
		bucket.contacts = append(bucket.contacts, contact)
		bucket.mu.Unlock()
	} else {
		bucket.updateContact(head)
	}
}

// Update the routing table with a new contact
func (node *KademliaNode) updateRoutingTable(addr string) {
	nodeID := hash(addr)
	contact := Contact{
		NodeID:   nodeID,
		Addr:     addr,
		LastSeen: time.Now(),
	}
	node.insertContact(contact)
}

// removeContact evicts a dead contact from the routing table.
func (node *KademliaNode) removeContact(addr string) {
	nodeID := hash(addr)
	idx := bucketIndex(node.NodeID, nodeID)
	if idx < 0 || idx >= KBucketCount {
		return
	}

	node.bucketsLock.RLock()
	bucket := node.buckets[idx]
	node.bucketsLock.RUnlock()

	bucket.mu.Lock()
	defer bucket.mu.Unlock()

	for i, c := range bucket.contacts {
		if c.Addr == addr {
			bucket.contacts = append(bucket.contacts[:i], bucket.contacts[i+1:]...)
			return
		}
	}
}

// Fetch the alpha-closest contacts to the target that are not visited
func (node *KademliaNode) getAlphaClosestContacts(shortlist []Contact,
	target [IDLength]byte, visited map[string]bool) []Contact {

	unqueried := []Contact{}
	for _, c := range shortlist {
		if !visited[c.Addr] && c.Addr != node.Addr {
			unqueried = append(unqueried, c)
		}
	}

	for i := 0; i < len(unqueried) && i < Alpha; i++ {
		for j := i + 1; j < len(unqueried); j++ {
			if xorDistance(unqueried[j].NodeID, target).Cmp(
				xorDistance(unqueried[i].NodeID, target)) < 0 {
				unqueried[i], unqueried[j] = unqueried[j], unqueried[i]
			}
		}
	}

	if len(unqueried) > Alpha {
		return unqueried[:Alpha]
	}
	return unqueried
}

// Sorts the contacts by their XOR distance to the target
func (node *KademliaNode) sortContactsByDistance(contacts []Contact, target [IDLength]byte) {
	sort.Slice(contacts, func(i, j int) bool {
		return xorDistance(contacts[i].NodeID, target).Cmp(
			xorDistance(contacts[j].NodeID, target)) < 0
	})
}

// Merge a new contact into the shortlist while keeping the length and order
func (node *KademliaNode) mergeShortlist(shortlist *[]Contact,
	newContact Contact, target [IDLength]byte, maxSize int) {
	for _, c := range *shortlist {
		if c.NodeID == newContact.NodeID {
			return
		}
	}

	contacts := append(*shortlist, newContact)
	node.sortContactsByDistance(contacts, target)
	if len(contacts) > maxSize {
		contacts = contacts[:maxSize]
	}
	*shortlist = contacts
}

// Check if the limit closest contacts to the target have been queried
func (node *KademliaNode) isLimitReached(shortlist []Contact,
	queried map[string]bool, target [IDLength]byte, limit int) bool {
	node.sortContactsByDistance(shortlist, target)
	if limit > len(shortlist) {
		return false
	}
	for i := 0; i < limit; i++ {
		if !queried[shortlist[i].Addr] {
			return false
		}
	}
	return true
}

// Core find algorithm of Kademlia protocol
func (node *KademliaNode) findNode(target [IDLength]byte) []Contact {
	shortlist := node.findClosestContacts(target, K)

	if len(shortlist) == 0 {
		return nil
	}

	queried := make(map[string]bool)

	for {
		candidates := node.getAlphaClosestContacts(shortlist, target, queried)
		if len(candidates) == 0 {
			break
		}

		// Query each candidate in parallel, tracking failures
		resultsCh := make(chan []Contact, len(candidates))
		var failedMu sync.Mutex
		failedAddrs := make(map[string]bool)
		var wg sync.WaitGroup
		for _, c := range candidates {
			queried[c.Addr] = true
			wg.Add(1)
			go func(contact Contact) {
				defer wg.Done()
				var reply []Contact
				err := node.RemoteCall(contact.Addr, "KademliaNode.FindNode", &target, &reply)
				if err == nil {
					resultsCh <- reply
				} else {
					resultsCh <- nil
					failedMu.Lock()
					failedAddrs[contact.Addr] = true
					failedMu.Unlock()
				}
			}(c)
		}

		wg.Wait()
		close(resultsCh)

		// Remove dead contacts from the shortlist so they don't
		// cause isLimitReached to stop the search prematurely.
		if len(failedAddrs) > 0 {
			alive := make([]Contact, 0, len(shortlist))
			for _, c := range shortlist {
				if !failedAddrs[c.Addr] {
					alive = append(alive, c)
				}
			}
			shortlist = alive
		}

		for result := range resultsCh {
			for _, c := range result {
				if c.Addr != node.Addr {
					node.mergeShortlist(&shortlist, c, target, K)
				}
			}
		}

		if node.isLimitReached(shortlist, queried, target, K) {
			break
		}
	}

	node.sortContactsByDistance(shortlist, target)
	if len(shortlist) > K {
		shortlist = shortlist[:K]
	}
	return shortlist
}

// Refresh a specific k-bucket by randomly selecting a target ID within range
func (node *KademliaNode) refreshKBucket(bucketIndex int) {
	node.bucketsLock.RLock()
	bucket := node.buckets[bucketIndex]
	node.bucketsLock.RUnlock()

	if bucket == nil {
		return
	}

	target := generateRandomID(node.NodeID, bucketIndex)
	contacts := node.findNode(target)
	for _, c := range contacts {
		node.insertContact(c)
	}
}

// Generate a random ID that falls within the range of the specified k-bucket.
// Bucket i covers XOR distances in [2^i, 2^(i+1) - 1].
// The random ID is produced by XOR-ing the local node's ID with a random
// distance drawn uniformly from that interval.
func generateRandomID(nodeID [IDLength]byte, bucketIndex int) [IDLength]byte {
	minDist := new(big.Int).Lsh(big.NewInt(1), uint(bucketIndex))
	maxDist := new(big.Int).Sub(new(big.Int).Lsh(big.NewInt(1), uint(bucketIndex+1)), big.NewInt(1))

	rangeSize := new(big.Int).Sub(maxDist, minDist)
	rangeSize.Add(rangeSize, big.NewInt(1))

	d, err := rand.Int(rand.Reader, rangeSize)
	if err != nil {
		return [IDLength]byte{}
	}
	d.Add(d, minDist)

	dBytes := d.Bytes()
	var target [IDLength]byte
	copy(target[IDLength-len(dBytes):], dBytes)
	for i := 0; i < IDLength; i++ {
		target[i] ^= nodeID[i]
	}
	return target
}

// Refresh all k-buckets periodically
func (node *KademliaNode) bucketRefreshLoop() {
	ticker := time.NewTicker(RefreshInterval)
	defer ticker.Stop()

	for {
		select {
		case <-ticker.C:
			for i := 0; i < KBucketCount; i++ {
				node.refreshKBucket(i)
			}
		case <-node.shutdown:
			return
		}
	}
}
