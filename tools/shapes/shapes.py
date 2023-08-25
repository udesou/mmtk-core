#!/usr/bin/env python3

# Please use libprotoc 24.1

from typing import Any, Dict, Tuple
import util.sanity.shapes_pb2 as shapes_pb2
import sys
import zstandard
from pathlib import Path
from collections import defaultdict
from multiprocessing import Pool


def parse_one(p: Path) -> Tuple[str, Dict[Any, Tuple[float, int]]]:
    bm_name = p.parent.name.split(".")[0]
    print(bm_name, file=sys.stderr)
    count = defaultdict(int)
    total = 0
    with zstandard.open(str(p), "rb") as fd:
        shapes_iter = shapes_pb2.ShapesIteration()
        shapes_iter.ParseFromString(fd.readall())
        for epoch in shapes_iter.epochs:
            for shape in epoch.shapes:
                if shape.kind is shapes_pb2.Shape.Kind.ValArray:
                    count["NoRef"] += 1
                elif shape.kind is shapes_pb2.Shape.Kind.ObjArray:
                    count["ObjArray"] += 1
                else:
                    if not shape.offsets:
                        count["NoRef"] += 1
                    else:
                        count[tuple(shape.offsets)] += 1
                total += 1
    # normalize it
    count = {pattern: (c / total, c) for pattern, c in count.items()}
    return bm_name, count


def tabulate(counts: Dict[str, Dict[Any, Tuple[float, int]]]):
    agg = defaultdict(float)
    for count in counts.values():
        for pattern, (norm, _) in count.items():
            agg[pattern] += norm
    agg = {pattern: s / len(counts) for pattern, s in agg.items()}
    bms = list(sorted(counts.keys()))
    print("rank\treference_pattern\tmean\tcumulative_mean\t{}".format("\t".join(bms)))
    cum_mean = 0
    for i, (pattern, mean) in enumerate(sorted(agg.items(), key=lambda x: -x[1]), 1):
        cum_mean += mean
        print("{}\t{}\t{:.2f}\t{:.2f}".format(
            i, pattern, mean*100, cum_mean*100), end="")
        for bm in bms:
            print("\t", end="")
            if pattern in counts[bm]:
                print("{:.2f}".format(counts[bm][pattern][0]*100), end="")

        print()


def main():
    counts: Dict[str, Dict[Any, float]]
    counts = {}
    with Pool(32) as p:
        results = p.map(parse_one, Path(sys.argv[1]).glob("*/shapes.binpb.zst"))
    for bm_name, count in results:
        if count:
            # actually have done a GC
            counts[bm_name] = count
    tabulate(counts)


if __name__ == "__main__":
    main()
