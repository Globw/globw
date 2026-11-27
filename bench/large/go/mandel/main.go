// Mandelbrot escape-time, like mandel.gw.
package main

import "fmt"

func main() {
	w, h, maxit := 2700, 1800, 200
	var total int64
	for py := 0; py < h; py++ {
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
			total += int64(i)
		}
	}
	fmt.Println(total)
}
