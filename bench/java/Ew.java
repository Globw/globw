// Like ew.gw: c = sqrt(a*a + b*b) over 20 M values, summed, 10 times. Plain loops, one thread.
public class Ew {
    public static void main(String[] x) {
        int n = x.length > 0 ? Integer.parseInt(x[0]) : 20000000;
        double[] a = new double[n], b = new double[n], c = new double[n];
        for (int i = 0; i < n; i++) a[i] = b[i] = i;
        double s = 0;
        for (int k = 0; k < 10; k++) {
            for (int i = 0; i < n; i++) c[i] = Math.sqrt(a[i] * a[i] + b[i] * b[i]);
            double t = 0; for (int i = 0; i < n; i++) t += c[i]; s += t;
        }
        System.out.println(s);
    }
}
