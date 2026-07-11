// This package implements a naive DHT protocol. (Actually, it is not distributed at all.)
// The performance and scalability of this protocol is terrible.
// You can use this as a reference to implement other protocols.
//
// In this naive protocol, the network is a complete graph, and each node stores all the key-value pairs.
// When a node joins the network, it will copy all the key-value pairs from another node.
// Any modification to the key-value pairs will be broadcasted to all the nodes.
// If any RPC call fails, we simply assume the target node is offline and remove it from the peer list.
package node

import (
	"net"
	"net/rpc"
	"os"
	"sync"
	"time"

	"github.com/sirupsen/logrus"
)

// Note: The init() function will be executed when this package is imported.
// See https://golang.org/doc/effective_go.html#init for more details.
func init() {
	// You can use the logrus package to print pretty logs.
	// Here we set the log output to a file.
	f, _ := os.Create("dht-test.log")
	logrus.SetOutput(f)
}

type Node struct {
	Addr   string // address and port number of the node, e.g., "localhost:1234"
	online bool

	listener  net.Listener
	server    *rpc.Server
	data      map[string]string
	dataLock  sync.RWMutex
	peers     map[string]struct{} // we use map as a set
	peersLock sync.RWMutex
}

// Pair is used to store a key-value pair.
// Note: It must be exported (i.e., Capitalized) so that it can be
// used as the argument type of RPC methods.
type Pair struct {
	Key   string
	Value string
}

// Initialize a node.
// Addr is the address and port number of the node, e.g., "localhost:1234".
func (node *Node) Init(addr string) {
	node.Addr = addr
	node.data = make(map[string]string)
	node.peers = make(map[string]struct{})
}

func (node *Node) RunRPCServer(wg *sync.WaitGroup) {
	node.server = rpc.NewServer()
	node.server.Register(node)
	var err error
	node.listener, err = net.Listen("tcp", node.Addr)
	wg.Done()
	if err != nil {
		logrus.Fatal("listen error: ", err)
	}
	for node.online {
		conn, err := node.listener.Accept()
		if err != nil {
			logrus.Error("accept error: ", err)
			return
		}
		go node.server.ServeConn(conn)
	}
}

func (node *Node) StopRPCServer() {
	node.online = false
	node.listener.Close()
}

// RemoteCall calls the RPC method at addr.
//
// Note: An empty interface can hold values of any type. (https://tour.golang.org/methods/14)
// Re-connect to the client every time can be slow. You can use connection pool to improve the performance.
func (node *Node) RemoteCall(addr string, method string, args interface{}, reply interface{}) error {
	if method != "Node.Ping" {
		logrus.Infof("[%s] RemoteCall %s %s %v", node.Addr, addr, method, args)
	}
	// Note: Here we use DialTimeout to set a timeout of 10 seconds.
	conn, err := net.DialTimeout("tcp", addr, 10*time.Second)
	if err != nil {
		logrus.Error("dialing: ", err)
		return err
	}
	client := rpc.NewClient(conn)
	defer client.Close()
	err = client.Call(method, args, reply)
	if err != nil {
		logrus.Error("RemoteCall error: ", err)
		return err
	}
	return nil
}

//
// RPC Methods
//

// Note: The methods used for RPC must be exported (i.e., Capitalized),
// and must have two arguments, both exported (or builtin) types.
// The second argument must be a pointer.
// The return type must be error.
// In short, the signature of the method must be:
//   func (t *T) MethodName(argType T1, replyType *T2) error
// See https://golang.org/pkg/net/rpc/ for more details.

// Here we use "_" to ignore the arguments we don't need.
// The empty struct "{}" is used to represent "void" in Go.

func (node *Node) GetData(_ string, reply *map[string]string) error {
	node.dataLock.RLock()
	*reply = node.data
	node.dataLock.RUnlock()
	return nil
}

func (node *Node) GetPeers(_ string, reply *map[string]struct{}) error {
	node.peersLock.RLock()
	*reply = node.peers
	node.peersLock.RUnlock()
	return nil
}

func (node *Node) AddPeer(addr string, _ *struct{}) error {
	node.peersLock.Lock()
	node.peers[addr] = struct{}{}
	node.peersLock.Unlock()
	return nil
}

func (node *Node) Ping(_ string, _ *struct{}) error {
	return nil
}

func (node *Node) RemovePeer(addr string, _ *struct{}) error {
	_, ok := node.peers[addr]
	if !ok {
		return nil
	}
	node.peersLock.Lock()
	delete(node.peers, addr)
	node.peersLock.Unlock()
	return nil
}

func (node *Node) PutPair(pair Pair, _ *struct{}) error {
	node.dataLock.Lock()
	node.data[pair.Key] = pair.Value
	node.dataLock.Unlock()
	return nil
}

func (node *Node) DeletePair(key string, _ *struct{}) error {
	node.dataLock.Lock()
	delete(node.data, key)
	node.dataLock.Unlock()
	return nil
}

//
// DHT methods
//

func (node *Node) Run(wg *sync.WaitGroup) {
	node.online = true
	go node.RunRPCServer(wg)
}

func (node *Node) Create() {
	logrus.Info("Create")
}

// Broadcast a RPC call to all the nodes in the network.
// If a node is offline, remove it from the peer list.
func (node *Node) broadcastCall(method string, args interface{}, reply interface{}) {
	offlinePeers := make([]string, 0)
	node.peersLock.RLock()
	for peer := range node.peers {
		err := node.RemoteCall(peer, method, args, reply)
		if err != nil {
			offlinePeers = append(offlinePeers, peer)
		}
	}
	node.peersLock.RUnlock()
	if len(offlinePeers) > 0 {
		node.peersLock.Lock()
		for _, peer := range offlinePeers {
			delete(node.peers, peer)
		}
		node.peersLock.Unlock()
	}
}

func (node *Node) Join(addr string) bool {
	logrus.Infof("Join %s", addr)
	// Copy data from the node at addr.
	node.dataLock.Lock()
	node.RemoteCall(addr, "Node.GetData", "", &node.data)
	node.dataLock.Unlock()
	// Copy the peer list from the node at addr.
	node.peersLock.Lock()
	node.RemoteCall(addr, "Node.GetPeers", "", &node.peers)
	node.peers[addr] = struct{}{}
	node.peersLock.Unlock()
	// Inform all the nodes in the network that a new node has joined.
	node.broadcastCall("Node.AddPeer", node.Addr, nil)
	return true
}

func (node *Node) Put(key string, value string) bool {
	logrus.Infof("Put %s %s", key, value)
	node.dataLock.Lock()
	node.data[key] = value
	node.dataLock.Unlock()
	// Broadcast the new key-value pair to all the nodes in the network.
	node.broadcastCall("Node.PutPair", Pair{key, value}, nil)
	return true
}

func (node *Node) Get(key string) (bool, string) {
	logrus.Infof("Get %s", key)
	node.dataLock.RLock()
	value, ok := node.data[key]
	node.dataLock.RUnlock()
	return ok, value
}

func (node *Node) Delete(key string) bool {
	logrus.Infof("Delete %s", key)
	// Check if the key exists.
	node.dataLock.RLock()
	_, ok := node.data[key]
	node.dataLock.RUnlock()
	if !ok {
		return false
	}
	// Delete the key-value pair.
	node.dataLock.Lock()
	delete(node.data, key)
	node.dataLock.Unlock()
	// Broadcast the deletion to all the nodes in the network.
	node.broadcastCall("Node.DeletePair", key, nil)
	return true
}

func (node *Node) Quit() {
	logrus.Infof("Quit %s", node.Addr)
	// Inform all the nodes in the network that this node is quitting.
	node.broadcastCall("Node.RemovePeer", node.Addr, nil)
	node.StopRPCServer()
}

func (node *Node) ForceQuit() {
	logrus.Info("ForceQuit")
	node.StopRPCServer()
}
