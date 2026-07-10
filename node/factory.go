package node

import "dht/kademlia"

func NewNode(port int) DhtNode {
	node := new(kademlia.KademliaNode)
	node.Init(portToAddr(localAddress, port))
	return node
}
