// Like sumb.gw, by hand on every core: goroutines over ranges, partial sums added.
package main

import (
	"fmt"
	"runtime"
	"sync"
)

func main() {
	const n = int64(2000000000)
	p := int64(runtime.NumCPU())
	sums := make([]int64, p)
	var wg sync.WaitGroup
	for t := int64(0); t < p; t++ {
		wg.Add(1)
		go func(t int64) {
			defer wg.Done()
			lo, hi := 1+n*t/p, n*(t+1)/p
			var s int64
			for i := lo; i <= hi; i++ {
				s += i % 7
			}
			sums[t] = s
		}(t)
	}
	wg.Wait()
	var s int64
	for _, x := range sums {
		s += x
	}
	fmt.Println(s)
}
