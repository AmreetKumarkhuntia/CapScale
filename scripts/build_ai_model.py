"""Wrap the RIFE model so that it takes and returns 8-bit YUV 4:2:0 planes.

RIFE itself wants two RGB pictures as floats. With the colour conversion and the
chroma resampling added to the model, that work runs on the GPU with the rest, and
the app only copies 8-bit planes in and out. Run by install-ai.ps1:

    uv run --no-project --with onnx python scripts/build_ai_model.py rife.onnx rife_yuv420.onnx

After rebuilding a model by hand, delete the `engines` folder next to it: what
TensorRT prepared from the old one would otherwise be used again.
"""
import sys

import numpy as np
import onnx
from onnx import TensorProto as T
from onnx import helper, numpy_helper

REST, MOVING = 3.0, 6.0


def wrap(src: str, dst: str) -> None:
    model = onnx.load(src)
    g = model.graph
    (rife_in,), (rife_out,) = [i.name for i in g.input], [o.name for o in g.output]
    nodes, consts = [], []

    def const(name, value, dtype=np.float32):
        consts.append(numpy_helper.from_array(np.asarray(value, dtype), name))
        return name

    def op(kind, inputs, out, **attrs):
        nodes.append(helper.make_node(kind, inputs, [out], name="yuv_" + out, **attrs))
        return out

    front, plane = const("yuv_front", [0, 1], np.int64), const("yuv_255", 255.0)
    zero, one = const("yuv_zero", 0.0), const("yuv_one", 1.0)

    def matrix(x, mat, offset, out):
        """Per pixel: out = x * mat + offset, on a [1, 3, h, w] picture."""
        rows = op("Transpose", [x], out + "_hwc", perm=[0, 2, 3, 1])
        moved = op("Add", [op("MatMul", [rows, mat], out + "_mul"), offset], out + "_add")
        return op("Transpose", [moved], out, perm=[0, 3, 1, 2])

    pictures = []
    for k in "01":
        y, u, v = (
            op("Unsqueeze", [op("Cast", [p + k], f"{p}{k}_f", to=T.FLOAT), front], f"{p}{k}_4") for p in "yuv"
        )
        chroma = op(
            "Resize",
            [op("Concat", [u, v], "c" + k, axis=1), "", const("yuv_twice" + k, [1, 1, 2, 2])],
            f"c{k}_up",
            mode="nearest",
            nearest_mode="floor",
            coordinate_transformation_mode="asymmetric",
        )
        yuv = op("Div", [op("Concat", [y, chroma], "yuv" + k, axis=1), plane], f"yuv{k}_n")
        pictures.append(op("Clip", [matrix(yuv, "to_rgb", "rgb_offset", "rgb" + k), zero, one], f"rgb{k}_c"))
    when = op("Add", [op("Mul", ["y0_4", zero], "when_0"), "t"], "when")
    nodes.append(helper.make_node("Concat", [*pictures, when], [rife_in], name="yuv_in", axis=1))
    before = len(nodes)

    size = op("Shape", ["y0"], "size")
    cut = op(
        "Slice",
        [rife_out, const("yuv_origin", [0, 0], np.int64), size, const("yuv_hw", [2, 3], np.int64)],
        "made",
    )
    yuv = matrix(op("Clip", [cut, zero, one], "made_c"), "to_yuv", "yuv_offset", "made_yuv")
    channel = const("yuv_channel", [1], np.int64)

    change = op("Abs", [op("Sub", ["y0_4", "y1_4"], "diff")], "change")
    cells = op("AveragePool", [change], "cells", kernel_shape=[8, 8], strides=[8, 8])
    spread = op("MaxPool", [cells], "spread", kernel_shape=[11, 11], pads=[5, 5, 5, 5])
    ramp = op("Div", [op("Sub", [spread, const("yuv_rest", REST)], "over"), const("yuv_ramp", MOVING - REST)], "ramp")
    gate = op("Max", [op("Clip", [ramp, zero, one], "ramp_c"), op("Sub", [one, "still"], "always")], "gate")
    moving = op("Resize", [gate, "", "", op("Shape", ["y0_4"], "size4")], "moving", mode="linear")
    moving_half = op("AveragePool", [moving], "moving_half", kernel_shape=[2, 2], strides=[2, 2])
    later = op("Cast", [op("GreaterOrEqual", ["t", const("yuv_mid", [0.5])], "later_b")], "later", to=T.FLOAT)

    def bytes_of(made, older, newer, weight, out, squeeze):
        real = op("Add", [older, op("Mul", [op("Sub", [newer, older], out + "_gap"), later], out + "_pick")], out + "_real")
        apart = op("Sub", [op("Mul", [made, plane], out + "_255"), real], out + "_apart")
        mixed = op("Add", [real, op("Mul", [apart, weight], out + "_part")], out + "_mix")
        up = op("Add", [mixed, const("yuv_half_" + out, 0.5)], out + "_r")
        whole = op("Clip", [op("Floor", [up], out + "_fl"), zero, plane], out + "_cl")
        op("Cast", [op("Squeeze", [whole, squeeze], out + "_sq")], out, to=T.UINT8)

    luma = op("Slice", [yuv, const("yuv_s0", [0], np.int64), const("yuv_e1", [1], np.int64), channel], "made_y")
    bytes_of(luma, "y0_4", "y1_4", moving, "y", front)
    both = op("Slice", [yuv, const("yuv_s1", [1], np.int64), const("yuv_e3", [3], np.int64), channel], "made_c2")
    half = op("AveragePool", [both], "made_half", kernel_shape=[2, 2], strides=[2, 2])
    bytes_of(half, "c0", "c1", moving_half, "uv", const("yuv_first", [0], np.int64))

    rife_nodes = list(g.node)
    del g.node[:], g.input[:], g.output[:], g.value_info[:]
    g.node.extend(nodes[:before] + rife_nodes + nodes[before:])
    g.initializer.extend(consts)
    for k in "01":
        g.input.append(helper.make_tensor_value_info("y" + k, T.UINT8, ["h", "w"]))
        g.input.extend(helper.make_tensor_value_info(p + k, T.UINT8, ["h2", "w2"]) for p in "uv")
    g.input.append(helper.make_tensor_value_info("t", T.FLOAT, [1]))
    g.input.append(helper.make_tensor_value_info("still", T.FLOAT, [1]))
    for name in ("to_rgb", "to_yuv"):
        g.input.append(helper.make_tensor_value_info(name, T.FLOAT, [3, 3]))
    for name in ("rgb_offset", "yuv_offset"):
        g.input.append(helper.make_tensor_value_info(name, T.FLOAT, [3]))
    g.output.append(helper.make_tensor_value_info("y", T.UINT8, ["h", "w"]))
    g.output.append(helper.make_tensor_value_info("uv", T.UINT8, [2, "h2", "w2"]))
    onnx.checker.check_model(model)
    onnx.save(model, dst)


if __name__ == "__main__":
    wrap(sys.argv[1], sys.argv[2])
    print("wrote", sys.argv[2])
