"""Gera as fixtures `rastreio-laco-<seq>.json` (teste `rastreio::laco::testes`): o laço de referência
(`p3a/referencia/laco_sicro.py`) com o VitTrack de referência em CPU, gravando CADA chamada ao rastreador
(a reprocura faz duas por quadro), e a decisão por quadro do laço por presença tirada de `p3a/res_laco`.

Rodar dentro de `p3a/`:
  ORT_DISABLE_TELEMETRY=1 .venv-gpu1311/bin/python -I <este arquivo> <pasta de saída>
"""
import json
import os
import sys

sys.path.insert(0, "referencia")
sys.path.insert(0, "src")
import laco_sicro as L  # noqa: E402
from bancada import sequencias  # noqa: E402
from rastreio_vittrack import VitTrack  # noqa: E402
from ver import quadros  # noqa: E402


class Gravador:
    def __init__(self, r):
        self.r, self.chamadas, self.quadro = r, [], None

    def iniciar(self, q, c):
        self.r.iniciar(q, c)

    def passo(self, q, caixa_de_busca=None, janela=True):
        caixa, conf = self.r.passo(q, caixa_de_busca, janela)
        self.chamadas.append([self.quadro, [float(v) for v in caixa_de_busca], bool(janela), [float(v) for v in caixa], float(conf)])
        return caixa, conf


def traj(am):
    return [[a.indice, None if a.caixa is None else [float(v) for v in a.caixa], float(a.conf), a.estado] for a in am]


def de_traj(t):
    return [L.Amostra(i, None if c is None else tuple(c), conf, e) for i, c, conf, e in t]


saida = sys.argv[1]
os.makedirs(saida, exist_ok=True)
seqs = sequencias()
for sn in sys.argv[2].split(",") if len(sys.argv) > 2 else list(seqs):
    video, gt = seqs[sn]
    qs = quadros(video)
    ini = next(k for k in range(len(gt)) if gt[k] and gt[k]["fracao"] >= 0.95)
    g = Gravador(VitTrack("dl/vittrack/object_tracking_vittrack_2023sep.onnx", False))

    def frames():
        for k in range(ini, len(qs)):
            g.quadro = k
            yield k, qs[k][:, :, ::-1]

    am, motivo, parou = L.seguir(g, frames(), gt[ini]["inteira"])
    ref = json.load(open(f"res_laco/{sn}__vittrack.json"))
    mt = json.load(open(f"res_laco/{sn}__edgetam.json"))
    igual = traj(am) == [[i, c, conf, e] for i, c, conf, e in ref["traj"]] and (motivo, parou) == (ref["motivo"], ref["parou_em"])
    out = {
        "seq": sn,
        "ini": ini,
        "n": len(qs) - ini,
        "caixa0": [float(v) for v in gt[ini]["inteira"]],
        "vittrack": {
            "igual_a_res_laco": igual,
            "chamadas": g.chamadas,
            "traj": traj(am),
            "motivo": motivo,
            "parou_em": parou,
            "chaves": L.quadros_chave(am),
        },
        "edgetam": {
            "traj": mt["traj"],
            "motivo": mt["motivo"],
            "parou_em": mt["parou_em"],
            "chaves": L.quadros_chave(de_traj(mt["traj"])),
        },
    }
    with open(os.path.join(saida, f"rastreio-laco-{sn}.json"), "w") as f:
        json.dump(out, f, separators=(",", ":"))
    print(sn, "chamadas", len(g.chamadas), "igual a res_laco", igual, flush=True)
