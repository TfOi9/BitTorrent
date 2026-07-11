package testutil

import (
	"math/rand"
	"os"
	"sync"
	"time"

	"github.com/fatih/color"
)

func init() {
	if os.Getenv("NO_COLOR") == "" {
		color.NoColor = false
	}
}

const (
	FirstPort int = 20000

	// Each in-process test owns a distinct, non-overlapping port range so the
	// whole package can be run in a single `go test ./node/...` invocation
	// without ports from a finished test clashing with the next one. The ranges
	// are kept below the ephemeral port range (see doc/env-setup.md).
	//   TestBasic            : 20000 .. 20100 (up to 101 nodes)
	//   TestForceQuit        : 20200 .. 20250 (up to  51 nodes)
	//   TestQuitAndStabilize : 20400 .. 20450 (up to  51 nodes)
	//   TestDelete           : 20600 .. 20620 (up to  21 nodes)
	BasicTestFirstPort  int = FirstPort       // 20000
	ForceQuitFirstPort  int = FirstPort + 200 // 20200
	QASFirstPort        int = FirstPort + 400 // 20400
	DeleteTestFirstPort int = FirstPort + 600 // 20600

	LengthOfKeyValue int = 50

	AfterTestSleepTime = 30 * time.Second

	BasicTestRoundNum               int     = 5
	BasicTestNodeSize               int     = 100
	BasicTestRoundJoinNodeSize      int     = 20
	BasicTestRoundQuitNodeSize      int     = 10
	BasicTestRoundPutSize           int     = 150
	BasicTestRoundGetSize           int     = 120
	BasicTestRoundDeleteSize        int     = 70
	BasicTestMaxFailRate            float64 = 0.01
	BasicTestAfterRunSleepTime              = 200 * time.Millisecond
	BasicTestJoinQuitSleepTime              = time.Second
	BasicTestAfterJoinQuitSleepTime         = 10 * time.Second

	ForceQuitNodeSize           int     = 50
	ForceQuitPutSize            int     = 500
	ForceQuitRoundNum           int     = 10
	ForceQuitMaxFailRate        float64 = 0.15
	ForceQuitRoundQuitNodeSize          = ForceQuitNodeSize / ForceQuitRoundNum
	ForceQuitAfterRunSleepTime          = 200 * time.Millisecond
	ForceQuitJoinSleepTime              = time.Second
	ForceQuitAfterJoinSleepTime         = 10 * time.Second
	ForceQuitFQSleepTime                = 500 * time.Millisecond

	QASNodeSize           int     = 50
	QASPutSize            int     = 500
	QASMaxFailRate        float64 = 0.01
	QASGetSize            int     = 20
	QASAfterRunSleepTime          = 200 * time.Millisecond
	QASJoinSleepTime              = time.Second
	QASAfterJoinSleepTime         = 10 * time.Second
	QASQuitSleepTime              = 80 * time.Millisecond
)

var (
	Green  = color.New(color.FgGreen)
	Red    = color.New(color.FgRed)
	Yellow = color.New(color.FgYellow)
	Cyan   = color.New(color.FgCyan)
)

var Wg *sync.WaitGroup

var letters = []rune("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ")

func RandString(length int) string {
	b := make([]rune, length)
	for i := range b {
		b[i] = letters[rand.Intn(len(letters))]
	}
	return string(b)
}

func RemoveFromArray(s []int, idx int) []int {
	s[len(s)-1], s[idx] = s[idx], s[len(s)-1]
	return s[:len(s)-1]
}

type TestInfo struct {
	Msg       string
	FailedCnt int
	TotalCnt  int
}

func (info *TestInfo) Success() {
	info.TotalCnt++
}

func (info *TestInfo) Fail() {
	info.TotalCnt++
	info.FailedCnt++
}

func (info *TestInfo) Finish(failedCnt *int, totalCnt *int) {
	*failedCnt += info.FailedCnt
	*totalCnt += info.TotalCnt
	info.printInfo()
}

func (info *TestInfo) printInfo() {
	if info.FailedCnt > 0 {
		Red.Printf("%s failed with error rate %.4f\n", info.Msg,
			float64(info.FailedCnt)/float64(info.TotalCnt))
	} else {
		Green.Printf("%s passed.\n", info.Msg)
	}
}
