#!/usr/bin/env python3

# Please use libprotoc 24.1

from typing import Any, Dict, Tuple
import util.sanity.shapes_pb2 as shapes_pb2
import sys
import zstandard
from pathlib import Path
from collections import defaultdict
from multiprocessing import Pool

VISIBILITIES = [2**x for x in range(6, 17)]

def parse_one(p: Path) -> Tuple[str, Dict[Any, Tuple[float, int]]]:
    bm_name = p.parent.name.split(".")[0]
    print(bm_name, file=sys.stderr)
    count = defaultdict(int)
    visible = {
        k: 0 for k in VISIBILITIES
    }
    invisible = {
        k: 0 for k in VISIBILITIES
    }
    total = 0
    with zstandard.open(str(p), "rb") as fd:
        shapes_iter = shapes_pb2.ShapesIteration()
        shapes_iter.ParseFromString(fd.readall())
        for epoch in shapes_iter.epochs:
            for shape in epoch.shapes:
                # print("{:x} {}".format(shape.object, shape.offsets))
                if shape.kind is shapes_pb2.Shape.Kind.ValArray:
                    count["NoRef"] += 1
                    for v in VISIBILITIES:
                        visible[v] += 1
                elif shape.kind is shapes_pb2.Shape.Kind.ObjArray:
                    count["ObjArray"] += 1
                    for v in VISIBILITIES:
                        # let's hope that object arrays are easier to scan
                        # collaboratively 
                        visible[v] += 1
                else:
                    if not shape.offsets:
                        count["NoRef"] += 1
                        for v in VISIBILITIES:
                            visible[v] += 1
                    else:
                        count[tuple(shape.offsets)] += 1
                        for v in VISIBILITIES:
                            if (shape.object // v) == ((shape.object + shape.offsets[-1]) // v):
                                visible[v] += 1
                            else:
                                invisible[v] += 1
                total += 1
    # normalize it
    count = {pattern: (c / total, c) for pattern, c in count.items()}
    return bm_name, count, visible, invisible


def tabulate(counts: Dict[str, Dict[Any, Tuple[float, int]]]):
    agg = defaultdict(float)
    for count in counts.values():
        for pattern, (norm, _) in count.items():
            agg[pattern] += norm
    agg = {pattern: s / len(counts) for pattern, s in agg.items()}
    bms = list(sorted(counts.keys()))
    with open("shapes.tsv", "w") as fd:
        fd.write("rank\treference_pattern\tmean\tcumulative_mean\t{}\n".format("\t".join(bms)))
        cum_mean = 0
        for i, (pattern, mean) in enumerate(sorted(agg.items(), key=lambda x: -x[1]), 1):
            cum_mean += mean
            fd.write("{}\t{}\t{:.2f}\t{:.2f}".format(
                i, pattern, mean*100, cum_mean*100))
            for bm in bms:
                fd.write("\t")
                if pattern in counts[bm]:
                    fd.write("{:.2f}".format(counts[bm][pattern][0]*100))
            fd.write("\n")

def tabulate_visibility(visibles: Dict[str, Dict[int, int]], invisibles: Dict[str, Dict[int, int]]):
    with open("visibility.tsv", "w") as fd:
        fd.write("benchmark\tvisibility\tvisible\tinvisible\tratio\n")
        for bm_name in visibles:
            visible = visibles[bm_name]
            invisible = invisibles[bm_name]
            for v in VISIBILITIES:
                fd.write("{}\t{}\t{}\t{}\t{}\n".format(bm_name, v, visible[v], invisible[v], invisible[v] / (visible[v] + invisible[v])))

def main():
    counts: Dict[str, Dict[Any, float]]
    counts = {}
    with Pool(32) as p:
        results = p.map(parse_one, Path(sys.argv[1]).glob("*/shapes.binpb.zst"))
    visibles = {}
    invisibles = {}
    for bm_name, count, visible, invisible in results:
        if count:
            # actually have done a GC
            counts[bm_name] = count
            visibles[bm_name] = visible
            invisibles[bm_name] = invisible
    tabulate(counts)
    tabulate_visibility(visibles, invisibles)

if __name__ == "__main__":
    main()
