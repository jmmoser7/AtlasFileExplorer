"""Local SAM 2 adapter. JSON lines in/out; image bytes never leave this process.

The executable is an isolated, user-installed environment. Runtime is offline;
scripts/setup-segmentation.ps1 owns installation and the one-time model download.
"""
import json
import os
import sys

os.environ["HF_HUB_OFFLINE"] = "1"
os.environ["TRANSFORMERS_OFFLINE"] = "1"
os.environ["HF_HUB_DISABLE_TELEMETRY"] = "1"

import cv2
import numpy as np
import torch
from sam2.build_sam import build_sam2
from sam2.sam2_image_predictor import SAM2ImagePredictor

torch.set_num_threads(max(1, min(4, (os.cpu_count() or 2) // 2)))
model_dir = os.environ["ATLAS_SEGMENT_MODEL"]
model = build_sam2("configs/sam2.1/sam2.1_hiera_t.yaml",
                   os.path.join(model_dir, "sam2.1_hiera_tiny.pt"),
                   device="cpu", apply_postprocessing=False)
predictor = SAM2ImagePredictor(model)
print('{"ready":true}', flush=True)
cached_key = None


def segment(request):
    global cached_key
    width, height = request["width"], request["height"]
    if (not isinstance(width, int) or not isinstance(height, int)
            or width < 2 or height < 2 or width * height > 768 * 768):
        raise ValueError("Invalid segmentation image size")
    rgb = request["rgb"]
    if len(rgb) != width * height * 3:
        raise ValueError("Invalid segmentation image pixels")
    prompts = request.get("points")
    legacy_protocol = prompts is None
    if prompts is None:
        # Keep the original point-only JSON protocol valid for older callers.
        prompts = [{"point": request["point"], "positive": True}]
    if (not prompts or not any(prompt.get("positive") is True for prompt in prompts)
            or any(not isinstance(prompt.get("positive"), bool)
                   or len(prompt.get("point", [])) != 2
                   or any(not np.isfinite(value) or value < 0 or value > 1
                          for value in prompt["point"])
                   for prompt in prompts)):
        raise ValueError("Segmentation needs finite in-image points and at least one positive point")
    for polygon in request["window"]:
        if any(not np.isfinite(value) or value < 0 or value > 1
               for point in polygon for value in point):
            raise ValueError("Invalid segmentation window geometry")
    key = (request["key"], width, height)
    points = np.array([[prompt["point"][0] * (width - 1),
                        prompt["point"][1] * (height - 1)]
                       for prompt in prompts], dtype=np.float32)
    labels = np.array([1 if prompt["positive"] else 0 for prompt in prompts],
                      dtype=np.int32)
    with torch.inference_mode():
        if cached_key != key:
            pixels = np.array(rgb, dtype=np.uint8).reshape(height, width, 3)
            predictor.set_image(pixels)
            cached_key = key
        masks, scores, _ = predictor.predict(point_coords=points,
                                             point_labels=labels, multimask_output=True)
    window = np.zeros_like(masks[0], dtype=np.uint8)
    polygons = [np.rint(np.array(c) * [width, height]).astype(np.int32)
                for c in request["window"] if len(c) >= 3]
    if not polygons:
        return {"contours": []} if legacy_protocol else {"candidates": []}
    cv2.fillPoly(window, polygons, 1)
    candidates = []
    for mask, score in zip(masks, scores):
        mask = mask.astype(np.uint8) & window
        contours, _ = cv2.findContours(mask, cv2.RETR_LIST, cv2.CHAIN_APPROX_SIMPLE)
        result = []
        # Even-odd fill preserves holes (arms, handles, etc.). Limit noise and payload.
        for contour in sorted(contours, key=cv2.contourArea, reverse=True)[:64]:
            if cv2.contourArea(contour) < 3:
                continue
            outline = cv2.approxPolyDP(contour, 0.65, True).reshape(-1, 2)
            if len(outline) >= 3:
                result.append([[float(x) / width, float(y) / height] for x, y in outline])
        candidates.append({"quality": float(score), "contours": result})
    if legacy_protocol:
        # Old point-only callers expect one contour set selected by SAM score.
        return {"contours": max(candidates, key=lambda candidate: candidate["quality"])["contours"]}
    return {"candidates": candidates}


for line in sys.stdin:
    try:
        reply = segment(json.loads(line))
    except Exception as error:
        reply = {"error": str(error)}
    print(json.dumps(reply, separators=(",", ":")), flush=True)
