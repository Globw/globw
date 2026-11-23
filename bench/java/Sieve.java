// Sieve of Eratosthenes up to n (default 10,000,000), like sieve.gw: prints the number of primes.
public class Sieve {
    public static void main(String[] a) {
        int n = a.length > 0 ? Integer.parseInt(a[0]) : 10000000;
        boolean[] composite = new boolean[n + 1];
        long primes = 0;
        for (long i = 2; i <= n; i++)
            if (!composite[(int) i]) for (long j = i * i; j <= n; j += i) composite[(int) j] = true;
        for (int i = 2; i <= n; i++) if (!composite[i]) primes++;
        System.out.println(primes);
    }
}
