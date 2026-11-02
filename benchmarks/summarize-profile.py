"""Summarize Chrome CPU sample time by function; does not measure GPU time."""
import collections
import json
import sys

with open(sys.argv[1]) as source:
    profile = json.load(source)
nodes = {node['id']: node for node in profile['nodes']}
functions = collections.Counter()
categories = collections.Counter()
for sample, duration in zip(profile['samples'], profile['timeDeltas']):
    frame = nodes[sample]['callFrame']
    name = frame['functionName']
    functions[name] += duration
    category = ('translated_guest' if name.startswith('guest_') else
                'compatibility_runtime' if 'runtime_bg.wasm' in frame['url'] else
                'other')
    categories[category] += duration
total = sum(categories.values())
print(json.dumps({
    'sampledSeconds': total / 1e6,
    'categoriesPercent': {name: round(time / total * 100, 2) for name, time in categories.items()},
    'topSelfTimeMs': [(name, round(time / 1000, 2)) for name, time in functions.most_common(30)],
}, indent=2))
