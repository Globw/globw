// Mandelbrot escape-time, like mandel.gw: 900 x 600, 200 iterations, sum of the counts.
public class Mandel {
    public static void main(String[] a) {
        int w = 900, h = 600, maxit = 200; long total = 0;
        if (a.length == 3) { w = Integer.parseInt(a[0]); h = Integer.parseInt(a[1]); maxit = Integer.parseInt(a[2]); }
        for (int py = 0; py < h; py++)
            for (int px = 0; px < w; px++) {
                double cr = px * 3.5 / w - 2.5, ci = py * 2.0 / h - 1.0, zr = 0, zi = 0;
                int i = 0;
                while (i < maxit && zr * zr + zi * zi <= 4.0) { double t = zr * zr - zi * zi + cr; zi = 2.0 * zr * zi + ci; zr = t; i++; }
                total += i;
            }
        System.out.println(total);
    }
}
