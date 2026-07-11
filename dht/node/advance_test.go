package node

import (
	"fmt"
	"math/rand"
	"sync"
	"testing"
	"time"

	"dht/testutil"
)

func forceQuitTest() (bool, int, int) {
	testutil.Yellow.Println("Start Force Quit Test")

	forceQuitFailedCnt, forceQuitTotalCnt, panicked := 0, 0, false

	defer func() {
		if r := recover(); r != nil {
			testutil.Red.Println("Program panicked with", r)
			panicked = true
		}
	}()

	nodes := new([testutil.ForceQuitNodeSize + 1]DhtNode)
	nodeAddresses := new([testutil.ForceQuitNodeSize + 1]string)
	kvMap := make(map[string]string)
	nodesInNetwork := make([]int, 0, testutil.BasicTestNodeSize+1)

	testutil.Wg = new(sync.WaitGroup)
	for i := 0; i <= testutil.ForceQuitNodeSize; i++ {
		nodes[i] = NewNode(testutil.ForceQuitFirstPort + i)
		nodeAddresses[i] = portToAddr(localAddress, testutil.ForceQuitFirstPort+i)

		testutil.Wg.Add(1)
		go nodes[i].Run(testutil.Wg)
	}

	testutil.Wg.Wait()
	time.Sleep(testutil.ForceQuitAfterRunSleepTime)

	joinInfo := testutil.TestInfo{
		Msg:       "Force quit join",
		FailedCnt: 0,
		TotalCnt:  0,
	}
	nodes[0].Create()
	nodesInNetwork = append(nodesInNetwork, 0)
	testutil.Cyan.Printf("Start joining\n")
	for i := 1; i <= testutil.ForceQuitNodeSize; i++ {
		addr := nodeAddresses[rand.Intn(i)]
		if !nodes[i].Join(addr) {
			joinInfo.Fail()
		} else {
			joinInfo.Success()
		}
		nodesInNetwork = append(nodesInNetwork, i)

		time.Sleep(testutil.ForceQuitJoinSleepTime)
	}
	joinInfo.Finish(&forceQuitFailedCnt, &forceQuitTotalCnt)

	time.Sleep(testutil.ForceQuitAfterJoinSleepTime)

	putInfo := testutil.TestInfo{
		Msg:       "Force quit put",
		FailedCnt: 0,
		TotalCnt:  0,
	}
	testutil.Cyan.Printf("Start putting\n")
	for i := 0; i < testutil.ForceQuitPutSize; i++ {
		key := testutil.RandString(testutil.LengthOfKeyValue)
		value := testutil.RandString(testutil.LengthOfKeyValue)
		kvMap[key] = value

		if !nodes[rand.Intn(testutil.ForceQuitNodeSize+1)].Put(key, value) {
			putInfo.Fail()
		} else {
			putInfo.Success()
		}
	}
	putInfo.Finish(&forceQuitFailedCnt, &forceQuitTotalCnt)

	for round := 1; round <= testutil.ForceQuitRoundNum-1; round++ {
		testutil.Cyan.Printf("Force Quit Round %d\n", round)

		testutil.Cyan.Printf("Start force quitting (round %d)\n", round)
		for i := 1; i <= testutil.ForceQuitRoundQuitNodeSize; i++ {
			idxInArray := rand.Intn(len(nodesInNetwork))

			nodes[nodesInNetwork[idxInArray]].ForceQuit()
			nodesInNetwork = testutil.RemoveFromArray(nodesInNetwork, idxInArray)

			time.Sleep(testutil.ForceQuitFQSleepTime)
		}

		getInfo := testutil.TestInfo{
			Msg:       fmt.Sprintf("Get (round %d)", round),
			FailedCnt: 0,
			TotalCnt:  0,
		}
		testutil.Cyan.Printf("Start getting (round %d)\n", round)
		for key, value := range kvMap {
			ok, res := nodes[nodesInNetwork[rand.Intn(len(nodesInNetwork))]].Get(key)
			if !ok || res != value {
				getInfo.Fail()
			} else {
				getInfo.Success()
			}
		}
		getInfo.Finish(&forceQuitFailedCnt, &forceQuitTotalCnt)
	}

	for i := 0; i <= testutil.ForceQuitNodeSize; i++ {
		nodes[i].Quit()
	}

	return panicked, forceQuitFailedCnt, forceQuitTotalCnt
}

func TestForceQuit(t *testing.T) {
	panicked, forceQuitFailedCnt, forceQuitTotalCnt := forceQuitTest()
	if panicked {
		t.Fatal("Force quit test panicked")
	}
	forceQuitFailRate := float64(forceQuitFailedCnt) / float64(forceQuitTotalCnt)
	if forceQuitFailRate > testutil.ForceQuitMaxFailRate {
		t.Errorf("Force quit test failed with fail rate %.4f", forceQuitFailRate)
	}
}

func quitAndStabilizeTest() (bool, int, int) {
	testutil.Yellow.Println("Start Quit & Stabilize Test")

	QASFailedCnt, QASTotalCnt, panicked := 0, 0, false

	defer func() {
		if r := recover(); r != nil {
			testutil.Red.Println("Program panicked with", r)
			panicked = true
		}
	}()

	nodes := new([testutil.QASNodeSize + 1]DhtNode)
	nodeAddresses := new([testutil.QASNodeSize + 1]string)
	kvMap := make(map[string]string)
	nodesInNetwork := make([]int, 0, testutil.QASNodeSize+1)

	testutil.Wg = new(sync.WaitGroup)
	for i := 0; i <= testutil.QASNodeSize; i++ {
		nodes[i] = NewNode(testutil.QASFirstPort + i)
		nodeAddresses[i] = portToAddr(localAddress, testutil.QASFirstPort+i)

		testutil.Wg.Add(1)
		go nodes[i].Run(testutil.Wg)
	}
	testutil.Wg.Wait()
	time.Sleep(testutil.QASAfterRunSleepTime)

	joinInfo := testutil.TestInfo{
		Msg:       "Quit & Stabilize join",
		FailedCnt: 0,
		TotalCnt:  0,
	}
	nodes[0].Create()
	nodesInNetwork = append(nodesInNetwork, 0)
	testutil.Cyan.Printf("Start joining\n")
	for i := 1; i <= testutil.QASNodeSize; i++ {
		addr := nodeAddresses[rand.Intn(i)]
		if !nodes[i].Join(addr) {
			joinInfo.Fail()
		} else {
			joinInfo.Success()
		}
		nodesInNetwork = append(nodesInNetwork, i)

		time.Sleep(testutil.QASJoinSleepTime)
	}
	joinInfo.Finish(&QASFailedCnt, &QASTotalCnt)

	time.Sleep(testutil.QASAfterJoinSleepTime)

	putInfo := testutil.TestInfo{
		Msg:       "Quit & Stabilize put",
		FailedCnt: 0,
		TotalCnt:  0,
	}
	testutil.Cyan.Printf("Start putting\n")
	for i := 0; i < testutil.QASPutSize; i++ {
		key := testutil.RandString(testutil.LengthOfKeyValue)
		value := testutil.RandString(testutil.LengthOfKeyValue)
		kvMap[key] = value

		if !nodes[rand.Intn(testutil.QASNodeSize+1)].Put(key, value) {
			putInfo.Fail()
		} else {
			putInfo.Success()
		}
	}
	putInfo.Finish(&QASFailedCnt, &QASTotalCnt)

	getInfo := testutil.TestInfo{
		Msg:       "Quit & Stabilize Quit",
		FailedCnt: 0,
		TotalCnt:  0,
	}
	for round := 1; round <= testutil.QASNodeSize; round++ {
		idxInArray := rand.Intn(len(nodesInNetwork))

		nodes[nodesInNetwork[idxInArray]].Quit()
		nodesInNetwork = testutil.RemoveFromArray(nodesInNetwork, idxInArray)

		time.Sleep(testutil.QASQuitSleepTime)

		getCnt := 0
		for key, value := range kvMap {
			ok, res := nodes[nodesInNetwork[rand.Intn(len(nodesInNetwork))]].Get(key)
			if !ok || res != value {
				getInfo.Fail()
			} else {
				getInfo.Success()
			}

			getCnt++
			if getCnt == testutil.QASGetSize {
				break
			}
		}
	}
	getInfo.Finish(&QASFailedCnt, &QASTotalCnt)

	for i := 0; i <= testutil.QASNodeSize; i++ {
		nodes[i].Quit()
	}

	return panicked, QASFailedCnt, QASTotalCnt
}

func TestQuitAndStabilize(t *testing.T) {
	panicked, QASFailedCnt, QASTotalCnt := quitAndStabilizeTest()
	if panicked {
		t.Fatal("Quit & Stabilize test panicked")
	}
	QASFailRate := float64(QASFailedCnt) / float64(QASTotalCnt)
	if QASFailRate > testutil.QASMaxFailRate {
		t.Errorf("Quit & Stabilize test failed with fail rate %.4f", QASFailRate)
	}
}
