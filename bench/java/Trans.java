// Like trans.gw: r = exp(a * 1e-7) + log(a) + sin(a) over 20 M values (a = 1..n), summed, 10 times.
public class Trans {
    public static void main(String[] x) {
        int n = x.length > 0 ? Integer.parseInt(x[0]) : 20000000;
        double[] a = new double[n], r = new double[n];
        for (int i = 0; i < n; i++) a[i] = i + 1.0;
        double s = 0;
        for (int k = 0; k < 10; k++) {
            for (int i = 0; i < n; i++) r[i] = Math.exp(a[i] * 0.0000001) + Math.log(a[i]) + Math.sin(a[i]);
            double t = 0; for (int i = 0; i < n; i++) t += r[i]; s += t;
        }
        System.out.println(Math.round(s));
    }
}
