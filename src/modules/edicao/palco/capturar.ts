/** `setPointerCapture` sem derrubar o gesto quando o ponteiro já não existe (toque cancelado, evento sintético). */
export function capturar(el: Element, pointerId: number): void {
  try {
    el.setPointerCapture(pointerId);
  } catch {
    /* sem captura o arrasto segue pelos eventos do próprio elemento */
  }
}
