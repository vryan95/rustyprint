"""Times rustyprint and WeasyPrint on the benchmark documents, one process per document.

    python bench/run.py <rustyprint.exe> <weasyprint.exe> <out-dir>

Each run starts a new process, as a command-line conversion does, so start-up time counts.
Peak memory is the process's peak working set. Needs `psutil`.
"""

import json
import os
import statistics
import subprocess
import sys
import time

import psutil

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)


def run(command):
    """Runs a command; returns its wall time and peak working set in MB."""
    start = time.perf_counter()
    process = psutil.Popen(command, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    peak = 0
    while process.poll() is None:
        try:
            info = process.memory_info()
            peak = max(peak, getattr(info, 'peak_wset', info.rss))
        except psutil.Error:
            pass
        time.sleep(0.005)
    elapsed = time.perf_counter() - start
    if process.returncode != 0:
        raise SystemExit(f'failed: {command}')
    return elapsed, peak / 1e6


def commands(rustyprint, weasyprint, html, out, margin_zero):
    rp = [rustyprint, html, '-o', os.path.join(out, 'rp.pdf')]
    if margin_zero:
        rp += ['--margin', '0']
    wp = [weasyprint, '-q', '--allowed-protocols', 'file']
    if margin_zero:
        wp += ['-s', os.path.join(HERE, 'noto.css')]
    wp += [html, os.path.join(out, 'wp.pdf')]
    return {'rustyprint': rp, 'weasyprint': wp}


def scenario(name, html, runs, tools, out, margin_zero=True):
    results = {}
    for tool, command in commands(*tools, html, out, margin_zero).items():
        run(command)  # warm-up: file caches and the like
        times, peaks = [], []
        start = time.perf_counter()
        for _ in range(runs):
            elapsed, peak = run(command)
            times.append(elapsed)
            peaks.append(peak)
        total = time.perf_counter() - start
        results[tool] = {
            'runs': runs,
            'total_s': round(total, 3),
            'median_s': round(statistics.median(times), 4),
            'min_s': round(min(times), 4),
            'max_s': round(max(times), 4),
            'peak_mb': round(max(peaks), 1),
        }
        print(name, tool, json.dumps(results[tool]), flush=True)
    return results


def main():
    rustyprint, weasyprint, out = sys.argv[1:4]
    os.makedirs(out, exist_ok=True)
    tools = (rustyprint, weasyprint)
    report = {
        'invoice': scenario('invoice',
                            os.path.join(ROOT, 'demos', 'samples', 'invoice', 'index.html'), 10,
                            tools, out, margin_zero=False),
        'report-500': scenario('report-500', os.path.join(HERE, 'report-long.html'), 3, tools,
                               out),
        'report-1x500': scenario('report-1x500', os.path.join(HERE, 'report-one.html'), 500,
                                 tools, out),
    }
    with open(os.path.join(out, 'results.json'), 'w') as f:
        json.dump(report, f, indent=2)


if __name__ == '__main__':
    main()
