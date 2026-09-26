"""Pre-computes the data Safelight's AI helper ships with (src-tauri/src/ai/clip_data.bin):

* CLIP ViT-B/32 text embeddings for the content-tag labels (zero-shot tagging), so
  the app never needs the text model or a tokenizer at runtime.
* LAION's aesthetic predictor (sa_0_4_vit_b_32_linear): a 512 -> 1 linear head on
  normalised CLIP image embeddings.

Usage: python scripts/gen_clip_data.py <dir with text_model_quantized.onnx, tokenizer.json, aesthetic.pth>
Sources: huggingface.co/Xenova/clip-vit-base-patch32, github.com/LAION-AI/aesthetic-predictor
"""
import struct
import sys
import zipfile
from pathlib import Path

import numpy as np
import onnxruntime as ort
from tokenizers import Tokenizer

# Label shown in the app -> prompt variants (averaged, as in the CLIP paper).
LABELS = {
    "people": ["people"], "portrait": ["a portrait of a person", "a headshot"],
    "group": ["a group of people posing"], "children": ["a child", "children playing"],
    "baby": ["a baby"], "couple": ["a couple"], "wedding": ["a wedding", "a bride and groom"],
    "crowd": ["a crowd of people"], "dog": ["a dog"], "cat": ["a cat"], "bird": ["a bird"],
    "horse": ["a horse"], "wildlife": ["a wild animal"], "insect": ["an insect"],
    "flowers": ["flowers"], "trees": ["trees"], "forest": ["a forest"], "landscape": ["a landscape"],
    "mountains": ["mountains"], "beach": ["a beach"], "sea": ["the ocean", "the sea"],
    "lake": ["a lake"], "river": ["a river"], "waterfall": ["a waterfall"],
    "sky": ["the sky", "clouds in the sky"], "sunset": ["a sunset", "a sunrise"],
    "night": ["a scene at night"], "snow": ["snow", "a winter scene"], "desert": ["a desert"],
    "city": ["a city", "a city skyline"], "street": ["a street"], "architecture": ["architecture", "a building"],
    "interior": ["the inside of a room"], "car": ["a car"], "motorbike": ["a motorcycle"],
    "bicycle": ["a bicycle"], "train": ["a train"], "aircraft": ["an airplane"], "boat": ["a boat"],
    "food": ["food", "a plate of food"], "drinks": ["a drink", "a cocktail"], "cake": ["a cake"],
    "sports": ["people playing sports", "an athlete"], "concert": ["a concert", "a band on stage"],
    "party": ["a party", "people celebrating"], "dancing": ["people dancing"], "fireworks": ["fireworks"],
    "document": ["a document with text", "a screenshot"], "product": ["a product shot"],
    "macro": ["a macro close-up"], "abstract": ["an abstract pattern"], "garden": ["a garden"],
    "park": ["a park"], "road": ["a road"], "wall": ["a wall"], "fence": ["a fence"],
}


def main(src: Path, out: Path) -> None:
    tok = Tokenizer.from_file(str(src / "tokenizer.json"))
    sess = ort.InferenceSession(str(src / "text_model_quantized.onnx"), providers=["CPUExecutionProvider"])
    names, embs = [], []
    for label, prompts in LABELS.items():
        texts = [f"a photo of {p}." for p in prompts]
        ids = [tok.encode(t).ids for t in texts]
        width = max(len(i) for i in ids)
        pad = tok.token_to_id("<|endoftext|>")
        batch = np.array([i + [pad] * (width - len(i)) for i in ids], dtype=np.int64)
        e = sess.run(["text_embeds"], {"input_ids": batch})[0]
        e /= np.linalg.norm(e, axis=1, keepdims=True)
        m = e.mean(axis=0)
        embs.append(m / np.linalg.norm(m))
        names.append(label)

    with zipfile.ZipFile(src / "aesthetic.pth") as z:
        w = np.frombuffer(z.read("archive/data/0"), dtype=np.float32)
        b = np.frombuffer(z.read("archive/data/1"), dtype=np.float32)
    assert w.shape == (512,) and b.shape == (1,), (w.shape, b.shape)

    # Layout (little endian): u32 n_labels, then per label u16 len + utf8 name,
    # then n_labels*512 f32 text embeddings, then 512 f32 aesthetic weights + 1 f32 bias.
    with open(out, "wb") as f:
        f.write(struct.pack("<I", len(names)))
        for n in names:
            raw = n.encode()
            f.write(struct.pack("<H", len(raw)) + raw)
        f.write(np.stack(embs).astype("<f4").tobytes())
        f.write(w.astype("<f4").tobytes())
        f.write(b.astype("<f4").tobytes())
    print(f"wrote {out} ({out.stat().st_size} bytes, {len(names)} labels)")


if __name__ == "__main__":
    root = Path(__file__).resolve().parent.parent
    main(Path(sys.argv[1]), root / "src-tauri" / "src" / "ai" / "clip_data.bin")
