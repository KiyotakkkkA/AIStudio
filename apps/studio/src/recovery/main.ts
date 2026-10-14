import "./style.css";

const query = new URLSearchParams(window.location.search);

for (const [id, key] of [
  ["message", "message"],
  ["backups", "backups"],
] as const) {
  const element = document.getElementById(id);
  if (element) element.textContent = query.get(key) ?? "";
}
