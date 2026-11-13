import "./style.css";

import { type BootProgress, describeProgress } from "./bootProgress";

declare global {
  interface Window {
    bootProgress?: (progress: BootProgress) => void;
  }
}

window.bootProgress = (progress) => {
  const view = describeProgress(progress);
  const label = document.getElementById("label");
  const bar = document.getElementById("bar");
  const amount = document.getElementById("amount");
  if (label) label.textContent = view.label;
  if (bar) bar.style.width = `${String(view.percent)}%`;
  if (amount) amount.textContent = view.amount;
};
