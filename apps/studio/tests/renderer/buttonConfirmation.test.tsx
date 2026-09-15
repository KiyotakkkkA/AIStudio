import assert from "node:assert/strict";
import { fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, test, vi } from "vitest";
import Button from "../../src/renderer/ui/atoms/buttons/Button";

beforeEach(() => {
  // jsdom has no layout; Modal filters focus targets by their rendered dimensions.
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(100);
});

afterEach(() => vi.restoreAllMocks());

const modalSetup = {
  title: "Удалить секрет?",
  content: <p>Это действие нельзя отменить.</p>,
  tone: "danger" as const,
};

test("confirmation defers the action, supports cancellation and invokes it once", () => {
  const onClick = vi.fn();
  render(
    <Button needConfirm modalSetup={modalSetup} onClick={onClick}>
      Удалить
    </Button>,
  );
  const trigger = screen.getByRole("button", { name: "Удалить" });
  trigger.focus();
  fireEvent.click(trigger);
  const dialog = screen.getByRole("dialog", { name: modalSetup.title });
  const cancel = within(dialog).getByRole("button", { name: "Отмена" });
  assert.ok(document.activeElement === cancel, "Cancel receives initial focus");
  assert.equal(onClick.mock.calls.length, 0);
  fireEvent.click(cancel);
  assert.equal(screen.queryByRole("dialog"), null);
  assert.ok(document.activeElement === trigger, "Focus returns to trigger");
  assert.equal(onClick.mock.calls.length, 0);
  fireEvent.click(trigger);
  fireEvent.keyDown(document, { key: "Escape" });
  assert.equal(screen.queryByRole("dialog"), null);
  assert.equal(onClick.mock.calls.length, 0);
  fireEvent.click(trigger);
  fireEvent.click(screen.getByRole("button", { name: "Подтвердить" }));
  assert.equal(onClick.mock.calls.length, 1);
  assert.equal(screen.queryByRole("dialog"), null);
});

test("submit buttons submit only after confirmation", () => {
  const onSubmit = vi.fn((event) => event.preventDefault());
  render(
    <form onSubmit={onSubmit}>
      <Button type="submit" needConfirm modalSetup={modalSetup}>
        Продолжить
      </Button>
    </form>,
  );
  fireEvent.click(screen.getByRole("button", { name: "Продолжить" }));
  assert.equal(onSubmit.mock.calls.length, 0);
  fireEvent.click(screen.getByRole("button", { name: "Подтвердить" }));
  assert.equal(onSubmit.mock.calls.length, 1);
});

test("disabled confirmation cannot execute an action", () => {
  const onClick = vi.fn();
  const view = render(
    <Button needConfirm modalSetup={modalSetup} onClick={onClick}>
      Удалить
    </Button>,
  );
  fireEvent.click(screen.getByRole("button", { name: "Удалить" }));
  view.rerender(
    <Button disabled needConfirm modalSetup={modalSetup} onClick={onClick}>
      Удалить
    </Button>,
  );
  fireEvent.click(screen.getByRole("button", { name: "Подтвердить" }));
  assert.equal(onClick.mock.calls.length, 0);
});

test("ordinary buttons still call their handler directly", () => {
  const onClick = vi.fn();
  render(<Button onClick={onClick}>Открыть</Button>);
  fireEvent.click(screen.getByRole("button", { name: "Открыть" }));
  assert.equal(onClick.mock.calls.length, 1);
  assert.equal(screen.queryByRole("dialog"), null);
});
