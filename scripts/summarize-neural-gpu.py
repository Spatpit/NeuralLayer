import argparse
import csv
import json
import math
from pathlib import Path

parser = argparse.ArgumentParser(description='Summarize per-stage GPU timestamps from --replay-app, excluding initialization.')
parser.add_argument('files', nargs='+', type=Path)
parser.add_argument('--skip', type=int, default=30, help='Skip this many source frames (overlay draws for older CSVs).')
args = parser.parse_args()
if args.skip < 0:
    parser.error('--skip must be nonnegative')
result = {}
for path in args.files:
    with path.open(newline='') as handle:
        reader = csv.DictReader(handle)
        if not reader.fieldnames or 'frame' not in reader.fieldnames:
            raise ValueError(f'Missing replay timing header: {path}')
        names = [x for x in reader.fieldnames if x.endswith('_ms')]
        warmup_column = 'source_frame' if 'source_frame' in reader.fieldnames else 'frame'
        rows = [r for r in reader if int(r[warmup_column]) >= args.skip]
    if not rows:
        raise ValueError(f'No measured frames: {path}')
    def summarize(name):
        values = sorted(float(r[name]) for r in rows)
        if any(not math.isfinite(v) or v < 0 for v in values):
            raise ValueError(f'Invalid timestamp: {path}, {name}')
        def percentile(p):
            index = (len(values)-1)*p
            low = math.floor(index)
            high = math.ceil(index)
            return values[low] + (values[high]-values[low])*(index-low)
        return {'mean':sum(values)/len(values), 'median':percentile(.5), 'p95':percentile(.95)}
    result[str(path)] = {'frames':len(rows), 'milliseconds':{name:summarize(name) for name in names}}
print(json.dumps(result,indent=2))
