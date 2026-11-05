import { fireEvent, render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { createIpcClient } from "@zvs/ipc";
import { contract, type Contract } from "@zvs/shared";
import { createFakeBridge } from "../../../../test/helpers/fakeBridge";
import ExecPolicyMenu from "../../src/renderer/features/execPolicy/ExecPolicyMenu";
import ExecPolicyStore from "../../src/renderer/features/execPolicy/ExecPolicyStore";
import {
  EXEC_MODE_COPY,
  EXEC_POLICY_LABEL,
  JUDGE_SETUP,
  NO_RESTRICTIONS_CONFIRM,
} from "../../src/renderer/features/execPolicy/execPolicyCopy";

function openMenu() {
  const store = new ExecPolicyStore(createIpcClient(contract, createFakeBridge<Contract>()));
  const setup = vi.fn();
  render(<ExecPolicyMenu store={store} setup={setup} />);
  fireEvent.click(
    screen.getByRole("button", {
      name: `${EXEC_POLICY_LABEL}: ${EXEC_MODE_COPY["always-ask"].label}`,
    }),
  );
  return { store, setup };
}

test("mode information opens without selecting the mode or closing the menu", async () => {
  const { store } = openMenu();
  const copy = EXEC_MODE_COPY["safety-ask"];
  const info = screen.getByRole("button", { name: `О режиме «${copy.label}»` });
  expect(info.closest('[role="menuitemradio"]')).toBeNull();
  fireEvent.mouseEnter(info);
  expect(await screen.findByText(copy.hint)).toBeTruthy();
  fireEvent.click(info);
  expect(store.mode).toBe("always-ask");
  expect(screen.getByRole("menu", { name: EXEC_POLICY_LABEL })).toBeTruthy();
});

test("unavailable automatic review keeps its setup action and mode information", () => {
  const { store, setup } = openMenu();
  expect(store.isAvailable("auto-bypass")).toBe(false);
  expect(
    screen.getByRole("button", {
      name: `О режиме «${EXEC_MODE_COPY["auto-bypass"].label}»`,
    }),
  ).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: JUDGE_SETUP }));
  expect(setup).toHaveBeenCalledOnce();
});

test("unrestricted execution still requires confirmation before selecting the mode", async () => {
  const { store } = openMenu();
  fireEvent.click(
    screen.getByRole("menuitemradio", {
      name: EXEC_MODE_COPY["no-restrictions"].label,
    }),
  );
  expect(await screen.findByRole("dialog", { name: NO_RESTRICTIONS_CONFIRM })).toBeTruthy();
  expect(store.mode).toBe("always-ask");
});
