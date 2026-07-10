package node

import (
	"math/rand"
	"sync"
	"testing"
	"time"

	"dht/testutil"
)

const (
	DeleteTestNodeSize    int     = 20
	DeleteTestPutSize     int     = 200
	DeleteTestMaxFailRate float64 = 0.01
)

func deleteTest() (bool, int, int) {
	testutil.Yellow.Println("Start Delete Test")

	deleteFailedCnt, deleteTotalCnt, panicked := 0, 0, false

	defer func() {
		if r := recover(); r != nil {
			testutil.Red.Println("Program panicked with", r)
			panicked = true
		}
	}()

	nodes := new([DeleteTestNodeSize + 1]DhtNode)
	nodeAddresses := new([DeleteTestNodeSize + 1]string)
	kvMap := make(map[string]string)
	nodesInNetwork := make([]int, 0, DeleteTestNodeSize+1)

	testutil.Wg = new(sync.WaitGroup)
	for i := 0; i <= DeleteTestNodeSize; i++ {
		nodes[i] = NewNode(testutil.DeleteTestFirstPort + i)
		nodeAddresses[i] = portToAddr(localAddress, testutil.DeleteTestFirstPort+i)

		testutil.Wg.Add(1)
		go nodes[i].Run(testutil.Wg)
	}
	testutil.Wg.Wait()
	time.Sleep(testutil.BasicTestAfterRunSleepTime)

	nodes[0].Create()
	nodesInNetwork = append(nodesInNetwork, 0)

	joinInfo := testutil.TestInfo{
		Msg:       "Delete test join",
		FailedCnt: 0,
		TotalCnt:  0,
	}
	testutil.Cyan.Printf("Start joining\n")
	for i := 1; i <= DeleteTestNodeSize; i++ {
		addr := nodeAddresses[nodesInNetwork[rand.Intn(len(nodesInNetwork))]]
		if !nodes[i].Join(addr) {
			joinInfo.Fail()
		} else {
			joinInfo.Success()
		}
		nodesInNetwork = append(nodesInNetwork, i)

		time.Sleep(testutil.BasicTestJoinQuitSleepTime)
	}
	joinInfo.Finish(&deleteFailedCnt, &deleteTotalCnt)

	time.Sleep(testutil.BasicTestAfterJoinQuitSleepTime)

	// Put the key-value pairs that will later be removed.
	putInfo := testutil.TestInfo{
		Msg:       "Delete test put",
		FailedCnt: 0,
		TotalCnt:  0,
	}
	testutil.Cyan.Printf("Start putting\n")
	for i := 0; i < DeleteTestPutSize; i++ {
		key := testutil.RandString(testutil.LengthOfKeyValue)
		value := testutil.RandString(testutil.LengthOfKeyValue)
		kvMap[key] = value

		if !nodes[nodesInNetwork[rand.Intn(len(nodesInNetwork))]].Put(key, value) {
			putInfo.Fail()
		} else {
			putInfo.Success()
		}
	}
	putInfo.Finish(&deleteFailedCnt, &deleteTotalCnt)

	// Remove every key-value pair; deleting an existing key must succeed.
	deleteInfo := testutil.TestInfo{
		Msg:       "Delete",
		FailedCnt: 0,
		TotalCnt:  0,
	}
	testutil.Cyan.Printf("Start deleting\n")
	for key := range kvMap {
		if !nodes[nodesInNetwork[rand.Intn(len(nodesInNetwork))]].Delete(key) {
			deleteInfo.Fail()
		} else {
			deleteInfo.Success()
		}
	}
	deleteInfo.Finish(&deleteFailedCnt, &deleteTotalCnt)

	// The removed pairs must no longer be retrievable from any node.
	getInfo := testutil.TestInfo{
		Msg:       "Get after delete",
		FailedCnt: 0,
		TotalCnt:  0,
	}
	testutil.Cyan.Printf("Start getting after delete\n")
	for key := range kvMap {
		ok, _ := nodes[nodesInNetwork[rand.Intn(len(nodesInNetwork))]].Get(key)
		if ok {
			getInfo.Fail()
		} else {
			getInfo.Success()
		}
	}
	getInfo.Finish(&deleteFailedCnt, &deleteTotalCnt)

	// Deleting an already-removed key must report failure.
	reDeleteInfo := testutil.TestInfo{
		Msg:       "Delete already-removed",
		FailedCnt: 0,
		TotalCnt:  0,
	}
	testutil.Cyan.Printf("Start deleting removed keys\n")
	for key := range kvMap {
		if nodes[nodesInNetwork[rand.Intn(len(nodesInNetwork))]].Delete(key) {
			reDeleteInfo.Fail()
		} else {
			reDeleteInfo.Success()
		}
	}
	reDeleteInfo.Finish(&deleteFailedCnt, &deleteTotalCnt)

	for i := 0; i <= DeleteTestNodeSize; i++ {
		nodes[i].Quit()
	}

	return panicked, deleteFailedCnt, deleteTotalCnt
}

func TestDelete(t *testing.T) {
	panicked, deleteFailedCnt, deleteTotalCnt := deleteTest()
	if panicked {
		t.Fatal("Delete test panicked")
	}
	deleteFailRate := float64(deleteFailedCnt) / float64(deleteTotalCnt)
	if deleteFailRate > DeleteTestMaxFailRate {
		t.Errorf("Delete test failed with fail rate %.4f", deleteFailRate)
	}
}
