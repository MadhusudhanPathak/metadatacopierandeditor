// Edit flow (spec 4.3, 7): Pick → Edit → Review → Save → Done.
import { api } from "../../api";
import { S } from "../../strings";
import { clear, stepper } from "../../ui";
import { renderEditor } from "./editor";
import { renderPick } from "./pick";
import { renderReview } from "./review";
import { mount, page, reset, st, STEP } from "./state";

function render() {
  clear(page());
  const head = stepper(S.steps.edit, st.step);
  if (st.step === STEP.pick) renderPick(head);
  else if (st.step === STEP.edit) renderEditor(head);
  else if (st.step === STEP.review) renderReview(head);
}

export async function editScreen(p: HTMLElement) {
  mount(p, render);
  reset();
  try {
    st.subfolders = (await api.getSettings()).includeSubfolders;
  } catch {
    // Keep the built-in default.
  }
  api
    .about()
    .then((a) => (st.canSetCreated = a.canSetCreated))
    .catch(() => {});
  render();
}
