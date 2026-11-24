// Like trans.gw: exp(a * 1e-7) + log(a) + sin(a) over 20 M values, summed, 10 times.
package main

import (
	"fmt"
	"math"
)

func main() {
	n := 20000000
	a, r := make([]float64, n), make([]float64, n)
	for i := range a {
		a[i] = float64(i) + 1.0
	}
	s := 0.0
	for k := 0; k < 10; k++ {
		for i := range r {
			r[i] = math.Exp(a[i]*0.0000001) + math.Log(a[i]) + math.Sin(a[i])
		}
		t := 0.0
		for _, v := range r {
			t += v
		}
		s += t
	}
	fmt.Printf("%.0f\n", math.Round(s))
}
