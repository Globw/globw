def main():
    w, h, maxit = 900, 600, 200
    total = 0
    for py in range(h):
        for px in range(w):
            cr = px * 3.5 / w - 2.5
            ci = py * 2.0 / h - 1.0
            zr = 0.0; zi = 0.0; i = 0
            while i < maxit and zr*zr + zi*zi <= 4.0:
                t = zr*zr - zi*zi + cr
                zi = 2.0*zr*zi + ci
                zr = t
                i += 1
            total += i
    print(total)
main()
