#!/usr/bin/env python3
"""Gera as imagens BMP do instalador NSIS do SICRO 2.0.

NSIS (modern UI) exige BMP nas dimensões exatas:
  - sidebar (welcome/finish): 164 x 314
  - header  (demais páginas):  150 x 57

Compoe os logos reais (sicro-logo.png + brasao-pca.png) sobre o fundo
navy da marca, com a wordmark em dourado. Reproduzível: rode
`python make_installer_art.py` a partir de sicro-desktop/src-tauri.

Saídas: installer/sidebar.bmp, installer/header.bmp (+ *_preview.png).
"""
from pathlib import Path
from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parent.parent.parent  # sicro-desktop/
OUT = Path(__file__).resolve().parent                 # src-tauri/installer/
LOGO = ROOT / "public" / "branding" / "sicro-logo.png"
BRASAO = ROOT / "public" / "branding" / "brasao-pca.png"

NAVY = (11, 22, 40)        # #0b1628
NAVY_CHIP = (24, 40, 66)   # #182842
GOLD = (215, 168, 79)      # #d7a84f
MUTED = (169, 182, 199)    # #a9b6c7
DIM = (113, 128, 150)      # #718096

FONT_SB = "C:/Windows/Fonts/seguisb.ttf"   # Segoe UI Semibold
FONT_RG = "C:/Windows/Fonts/segoeui.ttf"   # Segoe UI


def load_rgba(path: Path) -> Image.Image:
    return Image.open(path).convert("RGBA")


def fit(img: Image.Image, w=None, h=None) -> Image.Image:
    iw, ih = img.size
    if w and not h:
        h = round(ih * w / iw)
    elif h and not w:
        w = round(iw * h / ih)
    return img.resize((w, h), Image.LANCZOS)


def paste_center(base: Image.Image, top: Image.Image, cx: int, y: int) -> None:
    base.paste(top, (cx - top.width // 2, y), top)


def text_center(draw, cx, y, txt, font, fill):
    bb = draw.textbbox((0, 0), txt, font=font)
    draw.text((cx - (bb[2] - bb[0]) // 2, y), txt, font=font, fill=fill)


def build_sidebar() -> Image.Image:
    W, H = 164, 314
    img = Image.new("RGB", (W, H), NAVY)
    d = ImageDraw.Draw(img)
    cx = W // 2

    logo = fit(load_rgba(LOGO), w=82)
    paste_center(img, logo, cx, 24)

    text_center(d, cx, 116, "SICRO 2.0", ImageFont.truetype(FONT_SB, 27), GOLD)
    text_center(d, cx, 150, "Suíte Pericial Forense",
                ImageFont.truetype(FONT_RG, 12), MUTED)

    d.line([(42, 182), (W - 42, 182)], fill=(54, 80, 111), width=1)

    # Brasão da Polícia Científica num chip claro p/ ler bem sobre o navy.
    brasao = fit(load_rgba(BRASAO), h=58)
    chip_w, chip_h = brasao.width + 16, brasao.height + 12
    chip = Image.new("RGB", (chip_w, chip_h), NAVY_CHIP)
    img.paste(chip, (cx - chip_w // 2, 200))
    paste_center(img, brasao, cx, 206)

    foot_y = 200 + chip_h + 10
    text_center(d, cx, foot_y, "Polícia Científica",
                ImageFont.truetype(FONT_RG, 11), MUTED)
    text_center(d, cx, foot_y + 15, "do Amapá",
                ImageFont.truetype(FONT_RG, 11), DIM)
    return img


def build_header() -> Image.Image:
    W, H = 150, 57
    img = Image.new("RGB", (W, H), NAVY)
    d = ImageDraw.Draw(img)

    logo = fit(load_rgba(LOGO), h=38)
    img.paste(logo, (12, (H - logo.height) // 2), logo)

    tx = 12 + logo.width + 10
    d.text((tx, 12), "SICRO", font=ImageFont.truetype(FONT_SB, 19), fill=GOLD)
    d.text((tx, 33), "2.0", font=ImageFont.truetype(FONT_RG, 13), fill=MUTED)
    return img


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    side = build_sidebar()
    head = build_header()
    side.save(OUT / "sidebar.bmp")
    head.save(OUT / "header.bmp")
    # Previews PNG (não usados pelo NSIS; só p/ revisão visual).
    side.save(OUT / "sidebar_preview.png")
    head.save(OUT / "header_preview.png")
    print("OK: sidebar.bmp (164x314) + header.bmp (150x57) gerados em", OUT)


if __name__ == "__main__":
    main()
