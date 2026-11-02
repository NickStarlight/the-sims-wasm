"""Compare groups of community.mjs reports without mixing profiled timings.

Usage: python benchmarks/summarize-runs.py /tmp/pan-baseline-*.json -- /tmp/pan-candidate-*.json
"""
import json
import statistics
import sys

split = sys.argv.index('--')


def summarize(paths):
    reports = []
    for path in paths:
        with open(path) as source:
            report = json.load(source)
            if report.get('profiling'):
                raise ValueError(f'Exclude profiled timings from comparisons: {path}')
            reports.append(report)
    metrics = {
        'playToNeighborhoodMs': lambda r: r['playToNeighborhoodMs'],
        'lotLoadMs': lambda r: r['lotLoadMs'],
        'fps': lambda r: r['measurement']['gameFramesPerSecond'],
        'frameP95Ms': lambda r: r['measurement']['frameIntervalMs']['p95'],
    }
    return {
        'runs': len(reports),
        'metrics': {
            name: {'median': statistics.median(values), 'min': min(values), 'max': max(values)}
            for name, extract in metrics.items()
            for values in [[extract(report) for report in reports]]
        },
        'errors': [error for report in reports for error in report['errors']],
    }


print(json.dumps({'baseline': summarize(sys.argv[1:split]),
                  'candidate': summarize(sys.argv[split + 1:])}, indent=2))
