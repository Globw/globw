// Like matmul.gw: two 1200 x 1200 matrices of ones multiplied 10 times, the sum of each product (i-k-j loops).
public class Matmul {
    public static void main(String[] x) {
        int n = x.length > 0 ? Integer.parseInt(x[0]) : 1200;
        double[] a = new double[n * n], b = new double[n * n], c = new double[n * n];
        java.util.Arrays.fill(a, 1.0); java.util.Arrays.fill(b, 1.0);
        double s = 0;
        for (int k = 0; k < 10; k++) {
            java.util.Arrays.fill(c, 0.0);
            for (int i = 0; i < n; i++)
                for (int p = 0; p < n; p++) { double v = a[i * n + p]; for (int j = 0; j < n; j++) c[i * n + j] += v * b[p * n + j]; }
            double t = 0; for (double v : c) t += v; s += t;
        }
        System.out.println(s);
    }
}
