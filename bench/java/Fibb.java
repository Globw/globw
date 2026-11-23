// Naive recursive Fibonacci, like fibb.gw (n from the command line, default 35).
public class Fibb {
    static long fib(long n) { return n < 2 ? n : fib(n - 1) + fib(n - 2); }
    public static void main(String[] a) { System.out.println(fib(a.length > 0 ? Long.parseLong(a[0]) : 35)); }
}
