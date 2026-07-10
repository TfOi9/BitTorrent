// Package bridge provides a gRPC server that wraps the DHT node, exposing
// its operations to external clients (e.g., a Rust/C++ BitTorrent client).
package bridge

import (
	"context"
	"fmt"

	pb "dht/proto"

	"dht/kademlia"
	"dht/node"

	"github.com/sirupsen/logrus"
)

// DhtServer implements the generated DhtServiceServer gRPC interface.
// It wraps a DhtNode and translates between protobuf messages and
// the DhtNode's Go-native types.
type DhtServer struct {
	pb.UnimplementedDhtServiceServer
	node node.DhtNode
}

// NewDhtServer creates a new gRPC server wrapper around the given DHT node.
func NewDhtServer(n node.DhtNode) *DhtServer {
	return &DhtServer{node: n}
}

// ── Put ────────────────────────────────────────────────────────────────

func (s *DhtServer) Put(ctx context.Context, req *pb.PutRequest) (*pb.PutResponse, error) {
	logrus.Infof("[gRPC] Put key=%q", req.Key)
	ok := s.node.Put(req.Key, req.Value)
	return &pb.PutResponse{Success: ok}, nil
}

// ── Get ────────────────────────────────────────────────────────────────

func (s *DhtServer) Get(ctx context.Context, req *pb.GetRequest) (*pb.GetResponse, error) {
	logrus.Infof("[gRPC] Get key=%q", req.Key)
	found, value := s.node.Get(req.Key)
	return &pb.GetResponse{Found: found, Value: value}, nil
}

// ── Delete ─────────────────────────────────────────────────────────────

func (s *DhtServer) Delete(ctx context.Context, req *pb.DeleteRequest) (*pb.DeleteResponse, error) {
	logrus.Infof("[gRPC] Delete key=%q", req.Key)
	ok := s.node.Delete(req.Key)
	return &pb.DeleteResponse{Success: ok}, nil
}

// ── GetPeers ───────────────────────────────────────────────────────────

func (s *DhtServer) GetPeers(ctx context.Context, req *pb.GetPeersRequest) (*pb.GetPeersResponse, error) {
	if len(req.InfoHash) != 20 {
		return nil, fmt.Errorf("info_hash must be exactly 20 bytes, got %d", len(req.InfoHash))
	}

	// Convert raw bytes → hex string (the DhtNode interface uses hex).
	infoHashHex := fmt.Sprintf("%x", req.InfoHash)

	logrus.Infof("[gRPC] GetPeers info_hash=%s", infoHashHex[:16])

	peers, found := s.node.GetPeers(infoHashHex)

	resp := &pb.GetPeersResponse{
		Found: found,
	}
	if found {
		resp.Peers = make([]*pb.Peer, len(peers))
		for i, p := range peers {
			resp.Peers[i] = &pb.Peer{
				Ip:   p.IP,
				Port: uint32(p.Port),
			}
		}
	}

	return resp, nil
}

// ── AnnouncePeer ───────────────────────────────────────────────────────

func (s *DhtServer) AnnouncePeer(ctx context.Context, req *pb.AnnouncePeerRequest) (*pb.AnnouncePeerResponse, error) {
	if len(req.InfoHash) != 20 {
		return nil, fmt.Errorf("info_hash must be exactly 20 bytes, got %d", len(req.InfoHash))
	}

	infoHashHex := fmt.Sprintf("%x", req.InfoHash)
	peer := kademlia.Peer{
		IP:   req.Peer.Ip,
		Port: uint16(req.Peer.Port),
	}

	logrus.Infof("[gRPC] AnnouncePeer info_hash=%s peer=%s:%d",
		infoHashHex[:16], peer.IP, peer.Port)

	ok := s.node.AnnouncePeer(infoHashHex, peer)
	return &pb.AnnouncePeerResponse{Success: ok}, nil
}
