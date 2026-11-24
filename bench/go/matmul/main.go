// Like matmul.gw: two 1200 x 1200 matrices of ones multiplied 10 times (i-k-j order, one core).
package main

import (
	"fmt"
	"strconv"
)

func main() {
	n := 1200
	a, b, c := make([]float64, n*n), make([]float64, n*n), make([]float64, n*n)
	for i := range a {
		a[i], b[i] = 1, 1
	}
	s := 0.0
	for k := 0; k < 10; k++ {
		for i := range c {
			c[i] = 0
		}
		for i := 0; i < n; i++ {
			ci := c[i*n : i*n+n]
			for p := 0; p < n; p++ {
				x := a[i*n+p]
				bp := b[p*n : p*n+n]
				for j := range ci {
					ci[j] += x * bp[j]
				}
			}
		}
		t := 0.0
		for _, v := range c {
			t += v
		}
		s += t
	}
	fmt.Println(strconv.FormatFloat(s, 'g', 17, 64))
}
