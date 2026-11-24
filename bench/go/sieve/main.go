// Sieve of Eratosthenes up to 10 million, like sieve.gw.
package main

import "fmt"

func main() {
	n := 10000000
	prime := make([]bool, n+1)
	for i := 2; i <= n; i++ {
		prime[i] = true
	}
	for i := 2; i <= n; i++ {
		if prime[i] {
			for j := i * i; j <= n; j += i {
				prime[j] = false
			}
		}
	}
	primes := 0
	for i := 1; i <= n; i++ {
		if prime[i] {
			primes++
		}
	}
	fmt.Println(primes)
}
