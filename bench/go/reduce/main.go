// Like reduce.gw: sum + norm + mean of 50 M values, 20 times.
package main

import (
	"fmt"
	"math"
	"strconv"
)

func main() {
	n := 50000000
	a := make([]float64, n)
	for i := range a {
		a[i] = float64(i)
	}
	s := 0.0
	for k := 0; k < 20; k++ {
		sum, sq := 0.0, 0.0
		for _, v := range a {
			sum += v
		}
		for _, v := range a {
			sq += v * v
		}
		s += sum + math.Sqrt(sq) + sum/float64(n)
	}
	fmt.Println(strconv.FormatFloat(s, 'g', 17, 64))
}
