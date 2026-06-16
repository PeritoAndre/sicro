/**
 * Fase "Relatório" da Documentoscopia — a saída do exame, antes escondida
 * numa aba no fim do painel. Opera sobre o documento selecionado no Exame:
 *  - gera o relatório técnico (HTML + PDF best-effort, comando de backend);
 *  - oferece o quadro técnico curto para colar no laudo;
 *  - aponta onde os indícios/confrontos entram (Evidências).
 * §13: tudo é apoio técnico-computacional; a conclusão é do perito.
 */
import { useEffect, useMemo, useState } from "react";
import { AlertTriangle, ClipboardCopy, FileOutput, FileText } from "lucide-react";

import { Button } from "@components/Button/Button";
import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import {
  type DetectedField,
  type DocumentCaseFile,
  type OcrRun,
} from "@domain/documentoscopia";

import { buildTechnicalSummary } from "./relatorio";
import styles from "./DocumentoscopiaModule.module.css";

export function RelatorioView({
  ws,
  doc,
}: {
  ws: string;
  doc: DocumentCaseFile;
}) {
  const [fields, setFields] = useState<DetectedField[]>([]);
  const [runs, setRuns] = useState<OcrRun[]>([]);
  const [reportBusy, setReportBusy] = useState(false);
  const [reportMsg, setReportMsg] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    void (async () => {
      try {
        const [f, r] = await Promise.all([
          commands.listFields(ws, doc.id),
          commands.listOcrRuns(ws, doc.id),
        ]);
        if (!alive) return;
        setFields(f);
        setRuns(r);
      } catch (e) {
        if (alive) setErr(toSicroError(e).message);
      }
    })();
    return () => {
      alive = false;
    };
  }, [ws, doc.id]);

  const quadro = useMemo(
    () => buildTechnicalSummary(doc, fields, runs[0]),
    [doc, fields, runs],
  );

  const copy = (text: string) => void navigator.clipboard?.writeText(text);

  const genReport = async () => {
    setReportBusy(true);
    setReportMsg(null);
    setErr(null);
    try {
      const art = await commands.generateDocReport(ws, doc.id);
      const rel = art.pdf_relative_path ?? art.html_relative_path;
      await commands.revealEvidenceInFolder(ws, rel);
      setReportMsg(
        art.pdf_relative_path
          ? "Relatório gerado (HTML + PDF) — pasta aberta."
          : "Relatório gerado (HTML) — pasta aberta. PDF indisponível nesta máquina (Edge ausente).",
      );
    } catch (e) {
      setErr(toSicroError(e).message);
    } finally {
      setReportBusy(false);
    }
  };

  return (
    <div className={styles.relatorioWrap}>
      <div className={styles.relatorioHead}>
        <h2>
          <FileText size={18} aria-hidden /> Relatório técnico
        </h2>
        <p className={styles.relatorioSub}>
          Anexo do laudo para <strong>{doc.title}</strong>. Consolida
          proveniência (arquivo, hash, datas), OCR/texto revisado, campos,
          regiões/códigos e histórico — em linguagem indiciária (§13).
        </p>
      </div>

      {err && (
        <div className={styles.panelError}>
          <AlertTriangle size={13} /> {err}
        </div>
      )}

      <div className={styles.relatorioActions}>
        <Button
          variant="primary"
          leftIcon={<FileOutput size={15} />}
          onClick={() => void genReport()}
          disabled={reportBusy}
        >
          {reportBusy ? "Gerando…" : "Gerar relatório técnico (HTML/PDF)"}
        </Button>
        <Button
          variant="secondary"
          leftIcon={<ClipboardCopy size={15} />}
          onClick={() => copy(quadro)}
        >
          Copiar quadro técnico
        </Button>
      </div>

      {reportMsg && <p className={styles.hintLine}>{reportMsg}</p>}

      <p className={styles.hintLine}>
        Os <strong>indícios</strong> de manipulação digital (ELA, ruído,
        copy-move) e os <strong>confrontos</strong> entram no laudo pela aba
        Evidências, como peças indiciárias separadas. O quadro abaixo é a versão
        curta para colar diretamente no corpo do laudo.
      </p>

      <pre className={styles.quadro}>{quadro}</pre>
    </div>
  );
}
