// Copy flow (spec 4.2): Pick → Pair → Preview → Run → Done.
import { api } from "../../api";
import { S } from "../../strings";
import { clear, stepper } from "../../ui";
import { renderPair } from "./pair";
import { renderPick } from "./pick";
import { renderPreview } from "./preview";
import { mount, page, reset, st, STEP } from "./state";

function render() {
  clear(page());
  const head = stepper(S.steps.copy, st.step);
  if (st.step === STEP.pick) renderPick(head);
  else if (st.step === STEP.pair) renderPair(head);
  else if (st.step === STEP.preview) renderPreview(head);
}

export async function copyScreen(p: HTMLElement) {
  mount(p, render);
  reset();
  try {
    const settings = await api.getSettings();
    st.mode = settings.pairingMode;
    st.subfolders = settings.includeSubfolders;
  } catch {
    // Keep the built-in defaults.
  }
  api
    .about()
    .then((a) => (st.canSetCreated = a.canSetCreated))
    .catch(() => {});
  render();
}
