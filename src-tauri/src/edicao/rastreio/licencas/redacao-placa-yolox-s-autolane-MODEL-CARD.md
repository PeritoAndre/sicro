---
license: apache-2.0
tags:
  - object-detection
  - license-plate
  - yolox
  - alpr
datasets:
  - Autolane/us-license-plates
metrics:
  - map
  - map_50
pipeline_tag: object-detection
library_name: yolox
---

# YOLOX-S for US License Plate Detection

YOLOX-S fine-tuned for single-class license plate detection on ~20k US license plate images.

## Results

| Metric | Value |
|---|---|
| AP@50 | 98.9% |
| AP@50:95 | 90.1% |
| AP@75 | 97.6% |
| AR@50:95 | 92.5% |
| AP small | 75.7% |
| AP medium | 87.9% |
| AP large | 91.9% |
| Inference | ~4.2ms/image (FP16, fused) |

## Model Details

| Parameter | Value |
|---|---|
| Architecture | YOLOX-S (depth=0.33, width=0.50) |
| Classes | 1 (`license_plate`) |
| Input size | 640x640 |
| Pretrained from | COCO (`yolox_s.pth`) |
| Training epochs | 80 |
| Precision | FP16 |

## Training

See [Autolane/alpr-finetuning](https://github.com/Autolane/alpr-finetuning) for the full training pipeline.

W&B project: [goautolane/alpr-yolox](https://wandb.ai/goautolane/alpr-yolox)

### Hyperparameters

- Optimizer: SGD (momentum=0.9)
- Learning rate: 0.01/64 per image
- Weight decay: 5e-4
- Warmup: 5 epochs
- No-augmentation: last 10 epochs
- Augmentation: Mosaic, Mixup, HSV jitter, horizontal flip, rotation (+/-10deg)

## Quantized Models (QAT)

Quantization-Aware Training (QAT) fine-tunes the model with simulated int8 noise so weights become robust to quantization. Post-training quantization (PTQ) loses -7.8 AP50:95; QAT recovers most of this gap.

| Metric | FP32 | int8 PTQ | Delta |
|---|---|---|---|
| AP@50:95 | 0.901 | 0.823 | -7.8 |
| AP@50 | 0.989 | 0.989 | 0.0 |
| AP@75 | 0.976 | 0.966 | -1.0 |

### NPU Deployment (NXP i.MX93)

The Vela-compiled TFLite model targets the **Arm Ethos-U65-256** NPU on NXP i.MX93:

- **NPU utilization**: 96.2% (12 TRANSPOSE ops fall back to CPU)
- **Inference time**: ~120ms per image (Ethos-U65-256 @ 1GHz)
- **Model size**: 7.7MB (Vela) vs 35MB (ONNX FP32)

### ONNX Runtime Inference

```python
import onnxruntime as ort
import numpy as np

session = ort.InferenceSession("yolox_plates_s.onnx")
# Input: [1, 3, 640, 640] float32, range [0, 255]
input_name = session.get_inputs()[0].name
output = session.run(None, {input_name: image_tensor})
```

## Usage

```python
import torch
from yolox.exp import get_exp

# Load experiment config
exp = get_exp("yolox_plates_s.py", None)
model = exp.get_model()

# Load weights
ckpt = torch.load("best_ckpt.pth", map_location="cpu")
model.load_state_dict(ckpt["model"])
model.eval()
```

## Files

| File | Size | Description |
|---|---|---|
| `best_ckpt.pth` | 69MB | FP32 PyTorch checkpoint (best validation AP) |
| `yolox_plates_s.py` | — | Experiment config (architecture + training hyperparameters) |
| `yolox_plates_s.onnx` | 35MB | FP32 ONNX export (for ONNX Runtime inference) |
| `yolox_plates_s_int8.tflite` | 8.9MB | int8 TFLite via QAT (for TFLite Runtime / CPU) |
| `yolox_plates_s_vela.tflite` | 7.7MB | Vela-compiled int8 TFLite (for Ethos-U65-256 NPU) |

## Dataset

Trained on ~20k US license plate images (80/20 train/val split, stratified by US State). See [Autolane/us-license-plates](https://huggingface.co/datasets/Autolane/us-license-plates).
