package node

import "fmt"

var localAddress string

func init() {
	localAddress = "127.0.0.1"
}

func SetLocalAddress(addr string) {
	localAddress = addr
}

func portToAddr(ip string, port int) string {
	return fmt.Sprintf("%s:%d", ip, port)
}
