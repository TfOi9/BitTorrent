package node

import (
	"fmt"
	"math/rand"
	"sync"
	"testing"
	"time"

	"dht/testutil"
)

func basicTest() (bool, int, int) {
	basicFailedCnt, basicTotalCnt, panicked := 0, 0, false

	defer func() {
		if r := recover(); r != nil {
			testutil.Red.Println("Program panicked with", r)
			panicked = true
		}
	}()

	nodes := new([testutil.BasicTestNodeSize + 1]DhtNode)
	nodeAddresses := new([testutil.BasicTestNodeSize + 1]string)
	kvMap := make(map[string]string)

	testutil.Wg = new(sync.WaitGroup)
	for i := 0; i <= testutil.BasicTestNodeSize; i++ {
		nodes[i] = NewNode(testutil.BasicTestFirstPort + i)
		nodeAddresses[i] = portToAddr(localAddress, testutil.BasicTestFirstPort+i)

		testutil.Wg.Add(1)
		go nodes[i].Run(testutil.Wg)
	}

	testutil.Wg.Wait()

	nodesInNetwork := make([]int, 0, testutil.BasicTestNodeSize+1)

	time.Sleep(testutil.BasicTestAfterRunSleepTime)

	nodes[0].Create()
	nodesInNetwork = append(nodesInNetwork, 0)

	nextJoinNode := 1
	for round := 1; round <= testutil.BasicTestRoundNum; round++ {
		testutil.Cyan.Printf("Basic Test Round %d\n", round)

		joinInfo := testutil.TestInfo{
			Msg:       fmt.Sprintf("Join (round %d)", round),
			FailedCnt: 0,
			TotalCnt:  0,
		}
		testutil.Cyan.Printf("Start joining (round %d)\n", round)
		for j := 1; j <= testutil.BasicTestRoundJoinNodeSize; j++ {
			addr := nodeAddresses[nodesInNetwork[rand.Intn(len(nodesInNetwork))]]
			if !nodes[nextJoinNode].Join(addr) {
				joinInfo.Fail()
			} else {
				joinInfo.Success()
			}
			nodesInNetwork = append(nodesInNetwork, nextJoinNode)

			time.Sleep(testutil.BasicTestJoinQuitSleepTime)
			nextJoinNode++
		}
		joinInfo.Finish(&basicFailedCnt, &basicTotalCnt)

		time.Sleep(testutil.BasicTestAfterJoinQuitSleepTime)

		put1Info := testutil.TestInfo{
			Msg:       fmt.Sprintf("Put (round %d, part 1)", round),
			FailedCnt: 0,
			TotalCnt:  0,
		}
		testutil.Cyan.Printf("Start putting (round %d, part 1)\n", round)
		for i := 1; i <= testutil.BasicTestRoundPutSize; i++ {
			key := testutil.RandString(testutil.LengthOfKeyValue)
			value := testutil.RandString(testutil.LengthOfKeyValue)
			kvMap[key] = value

			if !nodes[nodesInNetwork[rand.Intn(len(nodesInNetwork))]].Put(key, value) {
				put1Info.Fail()
			} else {
				put1Info.Success()
			}
		}
		put1Info.Finish(&basicFailedCnt, &basicTotalCnt)

		get1Info := testutil.TestInfo{
			Msg:       fmt.Sprintf("Get (round %d, part 1)", round),
			FailedCnt: 0,
			TotalCnt:  0,
		}
		testutil.Cyan.Printf("Start getting (round %d, part 1)\n", round)
		get1Cnt := 0
		for key, value := range kvMap {
			ok, res := nodes[nodesInNetwork[rand.Intn(len(nodesInNetwork))]].Get(key)
			if !ok || res != value {
				get1Info.Fail()
			} else {
				get1Info.Success()
			}

			get1Cnt++
			if get1Cnt == testutil.BasicTestRoundGetSize {
				break
			}
		}
		get1Info.Finish(&basicFailedCnt, &basicTotalCnt)

		delete1Info := testutil.TestInfo{
			Msg:       fmt.Sprintf("Delete (round %d, part 1)", round),
			FailedCnt: 0,
			TotalCnt:  0,
		}
		testutil.Cyan.Printf("Start deleting (round %d, part 1)\n", round)
		for i := 1; i <= testutil.BasicTestRoundDeleteSize; i++ {
			for key := range kvMap {
				delete(kvMap, key)
				success := nodes[nodesInNetwork[rand.Intn(len(nodesInNetwork))]].Delete(key)
				if !success {
					delete1Info.Fail()
				} else {
					delete1Info.Success()
				}

				break
			}
		}
		delete1Info.Finish(&basicFailedCnt, &basicTotalCnt)

		testutil.Cyan.Printf("Start quitting (round %d)\n", round)
		for i := 1; i <= testutil.BasicTestRoundQuitNodeSize; i++ {
			idxInArray := rand.Intn(len(nodesInNetwork))

			nodes[nodesInNetwork[idxInArray]].Quit()
			nodesInNetwork = testutil.RemoveFromArray(nodesInNetwork, idxInArray)

			time.Sleep(testutil.BasicTestJoinQuitSleepTime)
		}
		testutil.Green.Printf("Quit (round %d) passed.\n", round)
		time.Sleep(testutil.BasicTestAfterJoinQuitSleepTime)

		put2Info := testutil.TestInfo{
			Msg:       fmt.Sprintf("Put (round %d, part 2)", round),
			FailedCnt: 0,
			TotalCnt:  0,
		}
		testutil.Cyan.Printf("Start putting (round %d, part 2)\n", round)
		for i := 1; i <= testutil.BasicTestRoundPutSize; i++ {
			key := testutil.RandString(testutil.LengthOfKeyValue)
			value := testutil.RandString(testutil.LengthOfKeyValue)
			kvMap[key] = value

			if !nodes[nodesInNetwork[rand.Intn(len(nodesInNetwork))]].Put(key, value) {
				put2Info.Fail()
			} else {
				put2Info.Success()
			}
		}
		put2Info.Finish(&basicFailedCnt, &basicTotalCnt)

		get2Info := testutil.TestInfo{
			Msg:       fmt.Sprintf("Get (round %d, part 2)", round),
			FailedCnt: 0,
			TotalCnt:  0,
		}
		testutil.Cyan.Printf("Start getting (round %d, part 2)\n", round)
		get2Cnt := 0
		for key, value := range kvMap {
			ok, res := nodes[nodesInNetwork[rand.Intn(len(nodesInNetwork))]].Get(key)
			if !ok || res != value {
				get2Info.Fail()
			} else {
				get2Info.Success()
			}

			get2Cnt++
			if get2Cnt == testutil.BasicTestRoundGetSize {
				break
			}
		}
		get2Info.Finish(&basicFailedCnt, &basicTotalCnt)

		delete2Info := testutil.TestInfo{
			Msg:       fmt.Sprintf("Delete (round %d, part 2)", round),
			FailedCnt: 0,
			TotalCnt:  0,
		}
		testutil.Cyan.Printf("Start deleting (round %d, part 2)\n", round)
		for i := 1; i <= testutil.BasicTestRoundDeleteSize; i++ {
			for key := range kvMap {
				delete(kvMap, key)
				success := nodes[nodesInNetwork[rand.Intn(len(nodesInNetwork))]].Delete(key)
				if !success {
					delete2Info.Fail()
				} else {
					delete2Info.Success()
				}

				break
			}
		}
		delete2Info.Finish(&basicFailedCnt, &basicTotalCnt)
	}

	for i := 0; i <= testutil.BasicTestNodeSize; i++ {
		nodes[i].Quit()
	}

	return panicked, basicFailedCnt, basicTotalCnt
}

func TestBasic(t *testing.T) {
	panicked, basicFailedCnt, basicTotalCnt := basicTest()
	if panicked {
		t.Fatal("Basic test panicked")
	}
	basicFailRate := float64(basicFailedCnt) / float64(basicTotalCnt)
	if basicFailRate > testutil.BasicTestMaxFailRate {
		t.Errorf("Basic test failed with fail rate %.4f", basicFailRate)
	}
}
