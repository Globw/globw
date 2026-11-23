// A tight counting loop, like sumb.gw: s += i mod 7 for i in 1..n (default 200,000,000).
public class Sumb {
    public static void main(String[] a) {
        long n = a.length > 0 ? Long.parseLong(a[0]) : 200000000L, s = 0;
        for (long i = 1; i <= n; i++) s += i % 7;
        System.out.println(s);
    }
}
