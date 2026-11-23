// Like reduce.gw: sum + norm + mean of 50 M values, 20 times.
public class Reduce {
    public static void main(String[] x) {
        int n = x.length > 0 ? Integer.parseInt(x[0]) : 50000000;
        double[] a = new double[n];
        for (int i = 0; i < n; i++) a[i] = i;
        double s = 0;
        for (int k = 0; k < 20; k++) {
            double sum = 0, sq = 0;
            for (int i = 0; i < n; i++) sum += a[i];
            for (int i = 0; i < n; i++) sq += a[i] * a[i];
            s += sum + Math.sqrt(sq) + sum / n;
        }
        System.out.println(s);
    }
}
