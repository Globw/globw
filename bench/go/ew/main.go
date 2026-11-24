// Like ew.gw: c = sqrt(a*a + b*b) over 20 M values, summed, 10 times. Plain loops, one core.
package main

import (
	"fmt"
	"math"
	"strconv"
)

func main() {
	n := 20000000
	a, b, c := make([]float64, n), make([]float64, n), make([]float64, n)
	for i := range a {
		a[i] = float64(i)
		b[i] = float64(i)
	}
	s := 0.0
	for k := 0; k < 10; k++ {
		for i := range c {
			c[i] = math.Sqrt(a[i]*a[i] + b[i]*b[i])
		}
		t := 0.0
		for _, v := range c {
			t += v
		}
		s += t
	}
	fmt.Println(strconv.FormatFloat(s, 'g', 17, 64))
}
