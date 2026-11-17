"""Gera `deteccao-referencia.json` (testes de `edicao::rastreio::detectar`): as saídas de `p3b/ref/detectores.py`
(onnxruntime 1.31.1 em CPU, a versão do SICRO) nos seis modelos da redação e no YOLOX-S COCO, nas mesmas imagens que
o teste lê do dossiê, e as coberturas de referência dos três conjuntos de `p3b/` (placas do Commons pelo `placas()`;
cabeças do CrowdHuman e rostos do WIDER a partir das saídas guardadas em `p3b/saida/`).
As imagens da conferência entram decodificadas pelo ffmpeg (`bgr24`), como o SICRO lê quadros: o decodificador de
JPEG do OpenCV e o do Rust diferem em ±1 nos pixels, o que muda caixas pequenas. O SHA-256 de cada quadro e de alguns
`cv2.resize` vão junto (o teste confere que os pixels de entrada e o redimensionamento são os mesmos).

Rodar dentro de `p3b/`:
  ORT_DISABLE_TELEMETRY=1 FFMPEG=/usr/bin/ffmpeg venv-cpu/bin/python -I <este arquivo> <saída.json>
"""
import hashlib
import json
import os
import subprocess
import sys

R = os.getcwd() + "/"
sys.path.insert(0, R + "ref")
sys.path.insert(0, R + "scripts")
import numpy as np  # noqa: E402
import cv2  # noqa: E402
import onnxruntime as ort  # noqa: E402
import detectores as D  # noqa: E402

M = R + "../modelos/redacao/"
COCO = R + "baixados/megvii/yolox_s.onnx"
IMAGENS = {
    "c060": "dados/placas_commons/img/c060.jpg",
    "c032": "dados/placas_commons/img/c032.jpg",
    "wider_12_28": "dados/widerface/WIDER_val/images/12--Group/12_Group_Group_12_Group_Group_12_28.jpg",
    "crowd_0": "dados/crowdhuman/Images/273271,c9db000d5146c15.jpg",
}


FFMPEG = os.environ.get("FFMPEG", "/usr/bin/ffmpeg")
# (largura, altura) dos redimensionamentos conferidos no primeiro quadro (o último é a redução exata de 2x)
TAMANHOS = [(1280, 736), (640, 640), (640, 480), (1056, 704), (2112, 1408), (960, 720)]


def quadro(caminho, w, h):
    """O quadro como o SICRO lê: ffmpeg, `rawvideo bgr24` (o teste faz o mesmo e confere o SHA-256)."""
    b = subprocess.run([FFMPEG, "-v", "error", "-i", caminho, "-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "bgr24", "-"], capture_output=True, check=True).stdout
    assert len(b) == w * h * 3, (caminho, len(b))
    return np.frombuffer(b, np.uint8).reshape(h, w, 3).copy()


def lista(d):
    return [[round(float(v), 4) for v in x[:5]] + [int(x[5])] for x in d]


def main(saida):
    assert os.environ.get("ORT_DISABLE_TELEMETRY") == "1"
    assert ort.__version__ == "1.31.1", ort.__version__
    ret = D.RetinaFace(M + "rosto_retinaface_r34.onnx")
    yun = D.YuNet(M + "rosto_yunet_n.onnx")
    cs = D.Yolox(M + "corpo_yolox_s_wholebody12.onnx", "esticar", True)
    cn = D.Yolox(M + "corpo_yolox_n_wholebody12.onnx", "esticar", True)
    rt = D.RtDetrPlaca(M + "placa_rtdetrv2_r18_openimages.onnx")
    yp = D.Yolox(M + "placa_yolox_s_autolane.onnx", "letterbox", True)
    coco = D.Yolox(COCO, "letterbox", True)
    imgs, casos, redim = [], [], []
    for nome, rel in IMAGENS.items():
        h, w = cv2.imread(R + rel).shape[:2]
        im = quadro(R + rel, w, h)
        imgs.append({"nome": nome, "arquivo": "p3b/" + rel, "w": w, "h": h, "bgr_sha256": hashlib.sha256(im.tobytes()).hexdigest()})
        if not redim:
            redim = [{"w": tw, "h": th, "bgr_sha256": hashlib.sha256(cv2.resize(im, (tw, th), interpolation=cv2.INTER_LINEAR).tobytes()).hexdigest()} for tw, th in TAMANHOS]

        def caso(modelo, params, d):
            casos.append({"modelo": modelo, "imagem": nome, "params": params, "dets": lista(d)})

        caso("rosto_retinaface_r34", {"thr": 0.3, "lado_max": 1024}, ret.detectar(im, 0.3, lado_max=1024))
        caso("rosto_yunet_n", {"thr": 0.35, "lado_max": 1024}, yun.detectar(im, 0.35, lado_max=1024))
        caso("corpo_yolox_s_wholebody12", {"thr": 0.15, "classes": [0, 2, 3], "hw": [736, 1280]}, cs.detectar(im, 0.15, classes=[0, 2, 3], hw=(736, 1280)))
        caso("corpo_yolox_n_wholebody12", {"thr": 0.15, "classes": [0, 2, 3], "hw": [736, 1280]}, cn.detectar(im, 0.15, classes=[0, 2, 3], hw=(736, 1280)))
        caso("placa_rtdetrv2_r18_openimages", {"thr": 0.045}, rt.detectar(im, 0.045))
        caso("placa_yolox_s_autolane", {"thr": 0.09}, yp.detectar(im, 0.09))
        caso("yolox_s_coco", {"thr": 0.35, "classes": list(D.VEICULOS_COCO)}, coco.detectar(im, 0.35, classes=list(D.VEICULOS_COCO)))
        # os pipelines do dossiê (scores normalizados)
        caso("cabecas", {"escala": 1.0}, D.cabecas(cs, im, 1.0))
        if not nome.startswith("c0"):
            caso("rostos_r34", {"escala": 2.0}, D.rostos(ret, im, 2.0))
            caso("rostos_yunet", {"escala": 1.5}, D.rostos(yun, im, 1.5))
        else:
            caso("placas", {}, D.placas(rt, yp, coco, im)[0])
            caso("placas_cpu", {}, D.placas(None, yp, coco, im)[0])
    versao = subprocess.run([FFMPEG, "-version"], capture_output=True, text=True).stdout.split()[2]
    json.dump({"gerado_por": "deteccao-gerar.py", "onnxruntime": ort.__version__, "provedor": "cpu", "ffmpeg": versao, "imagens": imgs, "redimensionar": redim, "casos": casos,
               "cobertura": cobertura(rt, yp, coco)}, open(saida, "w"), ensure_ascii=False, indent=0)


def cobertura(rt, yp, coco):
    """As medidas de `p3b/` que o teste reproduz: placas pelo `placas()` (inteiro + recorte, normalizado >= 0,5) nas 67
    fotos; cabeças (`pinto_s@1024`, thr 0,2, +15%) nas 800 do CrowdHuman; cabeça OU rosto R34 2x no WIDER."""
    import placas_medir as pm
    import cabecas_medir as cm
    gt = pm.gt
    fp = tp = 0
    for n, g in gt.items():
        im = cv2.imread(R + "dados/placas_commons/img/" + n)
        d = D.placas(rt, yp, coco, im)[0]
        d = d[d[:, 4] >= 0.5]
        usado = [False] * len(g["P"])
        for x in d:
            ious = [0 if usado[j] else pm.iou(x, p) for j, p in enumerate(g["P"])]
            j = int(np.argmax(ious)) if ious else -1
            if j >= 0 and ious[j] >= 0.4:
                usado[j] = True
                continue
            if any(pm.ioa(x, p) >= 0.5 or pm.iou(x, p) >= 0.2 for p in g["P"] + g["I"]):
                continue
            fp += 1
        tp += sum(usado)
    placas = {"placas": sum(len(g["P"]) for g in gt.values()), "achadas": tp, "fp": fp, "imagens": len(gt)}
    # cabeças: das saídas guardadas (GPU 1.32, `cabecas_rodar.py crowdhuman pinto_s@1024 800`)
    gch = cm.gt_crowdhuman()
    z = np.load(R + "saida/cabecas_crowdhuman/pinto_s@1024.npz")
    c = np.concatenate([cm.cobertura(gch[k], cm.expandir(z[k][(z[k][:, 5] == 1) & (z[k][:, 4] >= 0.2)][:, :4], 0.15)) for k in z.files])
    cabecas = {"imagens": len(z.files), "cabecas": int(len(c)), "cobertas": int((c >= 0.8).sum())}
    # cabeça OU rosto no WIDER (imagens com as duas saídas)
    gw = cm.gt_wider()
    zc = np.load(R + "saida/cabecas_wider/pinto_s@1024.npz")
    zr = np.load(R + "saida/wider/retina_r34@2048a.npz")
    comuns = sorted(set(zc.files) & set(zr.files))
    cs = []
    for k in comuns:
        reg = [cm.expandir(zc[k][(zc[k][:, 5] == 1) & (zc[k][:, 4] >= 0.2)][:, :4], 0.15), cm.expandir(zr[k][zr[k][:, 4] >= 0.67][:, :4], 0.25)]
        cs.append(cm.cobertura(gw[k], np.concatenate(reg)))
    c = np.concatenate(cs)
    rostos = {"imagens": len(comuns), "rostos": int(len(c)), "cobertos": int((c >= 0.8).sum()), "nomes": comuns}
    return {"placas": placas, "cabecas": cabecas, "rostos": rostos}


if __name__ == "__main__":
    main(sys.argv[1])
