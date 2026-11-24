// A tight counting loop, like sumb.gw.
package main

import "fmt"

func main() {
	var s int64
	for i := int64(1); i <= 200000000; i++ {
		s += i % 7
	}
	fmt.Println(s)
}
