// Like fill.gw: fill a float and a bool array of 10 M, count, 10 times. Plain loops, one core.
package main

import "fmt"

func main() {
	n := 10000000
	v := make([]float64, n+1)
	big := make([]bool, n+1)
	hits := 0
	for k := 1; k <= 100; k++ {
		for i := 1; i <= n; i++ {
			v[i] = float64(i%1000)*0.001 + float64(k)
			big[i] = (i*7919)%1000 > 500
		}
		for i := 1; i <= n; i++ {
			if big[i] {
				hits++
			}
		}
	}
	fmt.Println(hits, v[12345])
}
