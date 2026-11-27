// Like fill.gw, by hand on every core: each pass's ranges handed to goroutines.
package main

import (
	"fmt"
	"runtime"
	"sync"
)

func main() {
	n := 10000000
	v := make([]float64, n+1)
	big := make([]bool, n+1)
	p := runtime.NumCPU()
	hits := 0
	counts := make([]int, p)
	for k := 1; k <= 100; k++ {
		var wg sync.WaitGroup
		for t := 0; t < p; t++ {
			wg.Add(1)
			go func(t int) {
				defer wg.Done()
				lo, hi := 1+n*t/p, n*(t+1)/p
				for i := lo; i <= hi; i++ {
					v[i] = float64(i%1000)*0.001 + float64(k)
					big[i] = (i*7919)%1000 > 500
				}
				c := 0
				for i := lo; i <= hi; i++ {
					if big[i] {
						c++
					}
				}
				counts[t] = c
			}(t)
		}
		wg.Wait()
		for _, c := range counts {
			hits += c
		}
	}
	fmt.Println(hits, v[12345])
}
