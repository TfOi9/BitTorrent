//go:build ignore

package main

import (
	"fmt"
	"os"
	"sync"
	"time"

	"dht/node"
)

func main() {
	// Set local address for node identification
	node.SetLocalAddress("127.0.0.1")

	// Create two nodes on loopback (simulating Mac and Linux)
	nodeA := node.NewNode(21000) // "macOS"
	nodeB := node.NewNode(21001) // "Linux"

	var wg sync.WaitGroup

	// Start Node A
	wg.Add(1)
	go nodeA.Run(&wg)
	wg.Wait()

	// Start Node B
	wg.Add(1)
	go nodeB.Run(&wg)
	wg.Wait()

	// Node A creates the network
	nodeA.Create()
	time.Sleep(200 * time.Millisecond)

	// Node B joins Node A
	if !nodeB.Join("127.0.0.1:21000") {
		fmt.Println("FAIL: Node B failed to join Node A")
		os.Exit(1)
	}
	fmt.Println("Node B joined successfully")

	// Wait for routing tables to stabilize
	time.Sleep(2 * time.Second)

	// Step 1: Node A (macOS) puts greeting=macOS
	fmt.Println("\n=== Step 1: Node A puts greeting=macOS ===")
	ok := nodeA.Put("greeting", "macOS")
	fmt.Printf("Node A Put result: %v\n", ok)

	// Wait for propagation
	time.Sleep(500 * time.Millisecond)

	// Step 2: Node B (Linux) puts greeting=wsl
	fmt.Println("\n=== Step 2: Node B puts greeting=wsl ===")
	ok = nodeB.Put("greeting", "wsl")
	fmt.Printf("Node B Put result: %v\n", ok)

	// Wait for propagation
	time.Sleep(500 * time.Millisecond)

	// Step 3: Node B gets greeting - EXPECT wsl, but might get macOS
	fmt.Println("\n=== Step 3: Node B gets greeting ===")
	found, val := nodeB.Get("greeting")
	fmt.Printf("Node B Get greeting: found=%v, value=%q\n", found, val)

	if val == "macOS" {
		fmt.Println("\n*** BUG CONFIRMED: Node B got 'macOS' instead of 'wsl'! ***")
		fmt.Println("*** This is because Put() does not store locally, and Get() checks local first. ***")
	} else if val == "wsl" {
		fmt.Println("\nNode B got 'wsl' as expected.")
	}

	// Step 4: Also check what Node A has
	fmt.Println("\n=== Step 4: Node A gets greeting ===")
	found, val = nodeA.Get("greeting")
	fmt.Printf("Node A Get greeting: found=%v, value=%q\n", found, val)

	// Clean up
	nodeA.Quit()
	nodeB.Quit()
	time.Sleep(200 * time.Millisecond)
}
