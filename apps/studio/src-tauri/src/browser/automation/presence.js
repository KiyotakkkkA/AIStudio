export default function presence(command) {
  if (globalThis.top !== globalThis.window) return false;
  const id = "zvs-agent-presence";
  const doc = globalThis.document;
  const op = command?.op;
  const find = () => doc.getElementById(id);
  if (op === "remove") {
    find()?.remove();
    return true;
  }
  const styles = [
    ":host{--accent:#a3e635;--ink:#1a2e05}",
    ".frame{position:fixed;inset:0;pointer-events:none;box-shadow:inset 0 0 0 2px var(--accent),inset 0 0 22px 4px rgb(163 230 53/.45),inset 0 0 64px 12px rgb(163 230 53/.18);animation:breathe 2.4s ease-in-out infinite}",
    ".cursor{position:fixed;top:0;left:0;width:0;height:0;opacity:0;transition:transform 420ms cubic-bezier(.22,.8,.32,1),opacity 160ms ease-out;will-change:transform}",
    ".cursor.on{opacity:1}",
    ".arrow{position:absolute;top:-3px;left:-4px;overflow:visible;transform-origin:4px 3px;transition:transform 120ms ease-out;filter:drop-shadow(0 0 6px rgb(163 230 53/.7)) drop-shadow(0 2px 3px rgb(0 0 0/.35))}",
    ".arrow path{fill:var(--accent);stroke:var(--ink);stroke-width:1.4;stroke-linejoin:round}",
    ".cursor.down .arrow{transform:scale(.86)}",
    ".ripple{position:absolute;top:-18px;left:-18px;width:36px;height:36px;box-sizing:border-box;border-radius:999px;border:2px solid var(--accent);opacity:0}",
    ".ripple.go{animation:ripple 520ms ease-out forwards}",
    "@keyframes breathe{0%,100%{opacity:.55}50%{opacity:1}}",
    "@keyframes ripple{from{opacity:.9;transform:scale(.3)}to{opacity:0;transform:scale(1.4)}}",
    "@media (prefers-reduced-motion:reduce){.frame,.ripple.go{animation:none}.cursor{transition:none}}",
  ].join("");
  const arrow =
    '<svg class="arrow" width="26" height="26" viewBox="0 0 24 24">' +
    '<path d="M4 2.5 19.5 12 12.6 13.4 16.4 20.8 13.5 22.2 9.7 14.8 4.6 19.5Z"/></svg>';
  const mount = () => {
    const existing = find();
    if (existing !== null) return existing;
    const root = doc.documentElement;
    if (root === null) return null;
    const host = doc.createElement("zvs-agent-presence");
    host.id = id;
    host.setAttribute("aria-hidden", "true");
    host.style.cssText =
      "all:initial;display:block;position:fixed;inset:0;z-index:2147483647;pointer-events:none;";
    const shadow = host.attachShadow({ mode: "open" });
    shadow.innerHTML =
      `<style>${styles}</style><div class="frame"></div>` +
      `<div class="cursor"><span class="ripple"></span>${arrow}</div>`;
    root.appendChild(host);
    return host;
  };
  if (op === "show" && doc.documentElement === null) {
    doc.addEventListener("DOMContentLoaded", () => mount(), { once: true });
    return true;
  }
  const host = mount();
  if (host === null) return false;
  if (op === "hide") {
    host.style.visibility = "hidden";
    return true;
  }
  host.style.visibility = "visible";
  if (op !== "point") return true;
  const cursor = host.shadowRoot.querySelector(".cursor");
  const ripple = host.shadowRoot.querySelector(".ripple");
  const x = Number(command.x) || 0;
  const y = Number(command.y) || 0;
  const place = (left, top) => `translate(${left}px, ${top}px)`;
  if (!cursor.classList.contains("on")) {
    const from = Array.isArray(command.from) ? command.from : [x, y];
    cursor.style.transition = "none";
    cursor.style.transform = place(Number(from[0]) || 0, Number(from[1]) || 0);
    void cursor.offsetWidth;
    cursor.style.transition = "";
    cursor.classList.add("on");
  }
  cursor.style.transform = place(x, y);
  const still = globalThis.matchMedia("(prefers-reduced-motion: reduce)").matches;
  return new Promise((resolve) => {
    globalThis.setTimeout(
      () => {
        if (command.press) {
          ripple.classList.remove("go");
          void ripple.offsetWidth;
          ripple.classList.add("go");
          cursor.classList.add("down");
          globalThis.setTimeout(() => cursor.classList.remove("down"), 160);
        }
        resolve(true);
      },
      still ? 0 : 440,
    );
  });
}
