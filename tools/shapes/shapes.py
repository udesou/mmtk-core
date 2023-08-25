#!/usr/bin/env python3

# Please use libprotoc 24.1

import util.sanity.shapes_pb2 as shapes_pb2
import sys
from collections import defaultdict

count = defaultdict(int)
total_count = 0

with open(sys.argv[1], "rb") as f:
    shapes_iter = shapes_pb2.ShapesIteration()
    shapes_iter.ParseFromString(f.read())
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
            total_count += 1

cumulative = 0
for i, (offsets, count) in enumerate(sorted(count.items(), key=lambda x: -x[1]), 1):
    cumulative += count
    print("[{}] {}: {:.2%}, {:.2%}, {}".format(i, offsets,
          cumulative/total_count, count / total_count, count))
