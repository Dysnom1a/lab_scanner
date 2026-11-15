import json
import sys

import numpy as np
from PIL import Image, ImageOps
from rapidocr_onnxruntime import RapidOCR


def main() -> int:
    if len(sys.argv) < 2:
        sys.stderr.write("Usage: rapidocr_infer.py <image_path>\n")
        return 2

    image_path = sys.argv[1]
    with Image.open(image_path) as img:
        upright = ImageOps.exif_transpose(img).convert("RGB")
        orig_w, orig_h = upright.size
        scale = 2.0 if max(orig_w, orig_h) < 1600 else 1.0
        if scale > 1.0:
            upright = upright.resize(
                (int(orig_w * scale), int(orig_h * scale)),
                Image.Resampling.LANCZOS,
            )
        arr = np.array(upright)

    engine = RapidOCR()
    result, _ = engine(arr)

    blocks = []
    if result:
        for item in result:
            box, text, score = item[0], str(item[1]).strip(), float(item[2])
            if not text:
                continue
            blocks.append(
                {
                    "text": text,
                    "score": score,
                    "box": [
                        [float(pt[0]) / scale, float(pt[1]) / scale] for pt in box
                    ],
                }
            )

    sys.stdout.write(
        json.dumps(
            {
                "imageWidth": orig_w,
                "imageHeight": orig_h,
                "blocks": blocks,
            },
            ensure_ascii=False,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
