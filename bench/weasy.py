"""Renders an HTML file several times with WeasyPrint in one process and reports timings.

    python bench/weasy.py report.html 500 [out.pdf]

Relative URLs resolve against the file's directory; remote URLs are refused, as rustyprint
refuses them without an allow-list.
"""

import io
import sys
import time

import weasyprint
from weasyprint.urls import default_url_fetcher


def local_only(url, *args, **kwargs):
    if url.startswith(('http://', 'https://')):
        raise ValueError(f'remote URL refused: {url}')
    return default_url_fetcher(url, *args, **kwargs)


def main():
    path = sys.argv[1]
    runs = int(sys.argv[2]) if len(sys.argv) > 2 else 1
    out = sys.argv[3] if len(sys.argv) > 3 else None
    times = []
    start = time.perf_counter()
    pdf = b''
    pages = 0
    for _ in range(runs):
        t = time.perf_counter()
        document = weasyprint.HTML(filename=path, url_fetcher=local_only).render()
        pdf = document.write_pdf()
        times.append(time.perf_counter() - t)
        pages = len(document.pages)
    total = time.perf_counter() - start
    if out:
        with open(out, 'wb') as f:
            f.write(pdf)
    times.sort()
    print(f'weasyprint {weasyprint.__version__} runs={runs} pages={pages} bytes={len(pdf)} '
          f'total={total:.3f}s mean={total / runs:.4f}s median={times[len(times) // 2]:.4f}s '
          f'min={times[0]:.4f}s max={times[-1]:.4f}s')


if __name__ == '__main__':
    main()
