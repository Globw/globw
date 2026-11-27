// Like mandel.gw, by hand on every core: rows handed out to goroutines.
package main

import (
	"fmt"
	"runtime"
	"sync"
	"sync/atomic"
)

func main() {
	w, h, maxit := 2700, 1800, 200
	var next int64 = -1
	var total int64
	var wg sync.WaitGroup
	for t := 0; t < runtime.NumCPU(); t++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			var local int64
			for {
				py := int(atomic.AddInt64(&next, 1))
				if py >= h {
					break
				}
				for px := 0; px < w; px++ {
					cr := float64(px)*3.5/float64(w) - 2.5
					ci := float64(py)*2.0/float64(h) - 1.0
					zr, zi := 0.0, 0.0
					i := 0
					for i < maxit && zr*zr+zi*zi <= 4.0 {
						t := zr*zr - zi*zi + cr
						zi = 2.0*zr*zi + ci
						zr = t
						i++
					}
					local += int64(i)
				}
			}
			atomic.AddInt64(&total, local)
		}()
	}
	wg.Wait()
	fmt.Println(total)
}
