import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";

afterEach(() => {
  cleanup();
});

/**
 * jsdom implements no Popover API, and the uikit's Dropdown drives its menu through one:
 * `showPopover()` on open, a `toggle` event back, and `:popover-open` to read the state.
 * Chromium gives it all three, so this only fills the gap under test.
 *
 * `display` has to be set inline as well: jsdom does apply the UA rule that hides a closed
 * `[popover]`, but its own cascade can never match `:popover-open`, so an opened menu would
 * stay `display: none` and out of the accessibility tree. jsdom has no layout, so which
 * display an open popover claims here does not matter — only that it is not `none`.
 */
const shown = new WeakSet<Element>();
const restore = new WeakMap<Element, string>();

function popoverToggle(element: HTMLElement, open: boolean): void {
  element.dispatchEvent(new Event("toggle"));
  element.setAttribute("data-popover-open", String(open));
}

if (typeof HTMLElement.prototype.showPopover !== "function") {
  HTMLElement.prototype.showPopover = function showPopover(this: HTMLElement): void {
    if (shown.has(this)) return;
    shown.add(this);
    restore.set(this, this.style.display);
    this.style.display = "block";
    popoverToggle(this, true);
  };

  HTMLElement.prototype.hidePopover = function hidePopover(this: HTMLElement): void {
    if (!shown.delete(this)) return;
    this.style.display = restore.get(this) ?? "";
    popoverToggle(this, false);
  };

  HTMLElement.prototype.togglePopover = function togglePopover(this: HTMLElement): boolean {
    if (shown.has(this)) this.hidePopover();
    else this.showPopover();
    return shown.has(this);
  };

  const matches = Element.prototype.matches;
  Element.prototype.matches = function patched(this: Element, selectors: string): boolean {
    if (selectors === ":popover-open") return shown.has(this);
    return matches.call(this, selectors);
  } as typeof Element.prototype.matches;
}
