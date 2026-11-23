// Mandelbrot escape-time, like mandel.gw: 900 x 600, 200 iterations, sum of the counts.
#include <cstdio>
#include <cstdlib>
int main(int argc, char** argv) {
    int w = argc > 1 ? atoi(argv[1]) : 900, h = argc > 2 ? atoi(argv[2]) : 600, maxit = argc > 3 ? atoi(argv[3]) : 200;
    long long total = 0;
    for (int py = 0; py < h; py++)
        for (int px = 0; px < w; px++) {
            double cr = px * 3.5 / w - 2.5, ci = py * 2.0 / h - 1.0, zr = 0, zi = 0;
            int i = 0;
            while (i < maxit && zr * zr + zi * zi <= 4.0) {
                double t = zr * zr - zi * zi + cr;
                zi = 2.0 * zr * zi + ci;
                zr = t;
                i++;
            }
            total += i;
        }
    printf("%lld\n", total);
}
