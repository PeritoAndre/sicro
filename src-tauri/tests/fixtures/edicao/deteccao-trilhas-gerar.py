"""Gera `deteccao-trilhas.json` (teste `edicao::rastreio::detectar::testes::trilhas_como_a_referencia`): as propostas
de `p3b/ref/trilhas.py::trilhas_de_redacao` sobre as detecções por quadro guardadas em `p3b/saida/video/*.npz`, com o
score normalizado e os parâmetros que a Edição usa (alto 0,5, baixo 0,25, folga 25 % e piso 24 px, margem da classe).
Por proposta: primeiro e último quadro, linhas por origem e a caixa no primeiro, no do meio e no último.

Rodar dentro de `p3b/`:
  venv-cpu/bin/python -I <este arquivo> <saída.json>
"""
import json
import os
import sys

R = os.getcwd() + "/"
sys.path.insert(0, R + "ref")
import numpy as np  # noqa: E402
import trilhas as T  # noqa: E402

# (arquivo, classe no npz, alto do modelo para normalizar (None = já normalizado), margem, piso da margem)
CASOS = {
    "loja_cabecas": ("saida/video/loja_corpo_s.npz", 1, 0.35, 0.15, 2.0),
    "loja_rostos": ("saida/video/loja_rosto_yunet.npz", 2, 0.70, 0.25, 2.0),
    "ezymart_cabecas": ("saida/video/ezymart_corpo_s.npz", 1, 0.35, 0.15, 2.0),
    "estacionamento_placas": ("saida/video/est_zoom_placas.npz", 4, None, 0.15, 3.0),
}


def main(saida):
    out = {}
    for nome, (arq, classe, alto, mf, mp) in CASOS.items():
        z = np.load(R + arq)
        fps = float(z["fps"])
        H, W = (int(v) for v in z["forma"])
        qs = sorted(k for k in z.files if k.startswith("q"))
        dets = []
        for k in qs:
            d = z[k][z[k][:, 5] == classe][:, :5].copy()
            if alto is not None:
                d[:, 4] = np.minimum(1.0, 0.5 * d[:, 4] / alto)
            dets.append(d)
        props = T.trilhas_de_redacao(dets, fps, W, H, 0.5, 0.25, extra_nova=0.1, folga=0.25, piso=24, margem_frac=mf, margem_px=mp)
        r = lambda v: [round(float(x), 3) for x in v]
        out[nome] = {
            "arquivo": "p3b/" + arq, "classe": classe, "alto": alto, "margem": mf, "margem_px": mp, "fps": fps, "w": W, "h": H, "quadros": len(qs),
            "propostas": [{"de": int(p[0, 0]), "ate": int(p[-1, 0]), "n": len(p), "origem": [int((p[:, 6] == o).sum()) for o in (0, 1, 2)],
                           "primeira": r(p[0, 1:5]), "meio": r(p[len(p) // 2, 1:5]), "ultima": r(p[-1, 1:5]), "meio_q": int(p[len(p) // 2, 0])} for p in props],
        }
        print(nome, len(props))
    json.dump(out, open(saida, "w"), ensure_ascii=False, indent=0)


if __name__ == "__main__":
    main(sys.argv[1])
