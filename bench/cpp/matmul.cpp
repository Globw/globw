// Like matmul.gw: two 1200 x 1200 matrices of ones multiplied 10 times, the sum of each product.
// The usual i-k-j loop order, one core.
#include <cstdio>
#include <cstdlib>
#include <vector>
int main(int argc, char** argv) {
    size_t n = argc > 1 ? strtoull(argv[1], 0, 10) : 1200;
    std::vector<double> a(n * n, 1.0), b(n * n, 1.0), c(n * n);
    double s = 0;
    for (int k = 0; k < 10; k++) {
        std::fill(c.begin(), c.end(), 0.0);
        for (size_t i = 0; i < n; i++)
            for (size_t p = 0; p < n; p++) { double x = a[i * n + p]; for (size_t j = 0; j < n; j++) c[i * n + j] += x * b[p * n + j]; }
        double t = 0; for (double v : c) t += v; s += t;
    }
    printf("%.17g\n", s);
}
