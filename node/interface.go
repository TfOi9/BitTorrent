package node

import (
	"dht/kademlia"
	"sync"
)

type DhtNode interface {
	Run(waitgroup *sync.WaitGroup)

	Create()
	Join(addr string) bool

	Quit()
	ForceQuit()

	Put(key string, value string) bool
	Get(key string) (bool, string)
	Delete(key string) bool

	// GetPeers finds peers for a given info_hash (hex-encoded, 40 chars).
	// Returns the peer list and whether any were found.
	GetPeers(infoHash string) ([]kademlia.Peer, bool)

	// AnnouncePeer registers a peer as a downloader/seeder for the
	// given info_hash (hex-encoded). Returns true on success.
	AnnouncePeer(infoHash string, peer kademlia.Peer) bool
}
