import { observer } from "mobx-react-lite";
import { useEffect, useState } from "react";
import { mdiAlertOutline, mdiClose } from "@mdi/js";
import Button from "../../ui/atoms/buttons/Button";
import IconButton from "../../ui/atoms/buttons/IconButton";
import Icon from "../../ui/atoms/Icon";
import ConfirmModal from "../../ui/molecules/ConfirmModal";
import { errorCopy } from "../secrets/errorCopy";
import type ExecPolicyStore from "./ExecPolicyStore";
import {
  PACK_CONFIRM,
  PACK_CONFIRM_CONSEQUENCE,
  PACK_CONFIRM_TITLE,
  RULES_EMPTY,
  RULES_FAILED,
  RULES_LOADING,
  RULE_EFFECT_LABEL,
  RULE_INACTIVE,
  RULE_REMOVE_LABEL,
  ruleCreated,
  unconfirmedPack,
} from "./rulesCopy";

function RuleList({ store }: { readonly store: ExecPolicyStore }) {
  const [confirming, setConfirming] = useState<string | null>(null);
  useEffect(() => {
    void store.loadRules();
  }, [store]);
  if (store.rulesState === "idle" || (store.rulesState === "loading" && store.rules.length === 0))
    return <p className="m-0 text-[12.5px] text-main-500">{RULES_LOADING}</p>;
  if (store.rulesState === "failed")
    return <p className="m-0 text-[12.5px] text-err">{RULES_FAILED}</p>;
  return (
    <div className="flex flex-col gap-2">
      {store.unconfirmed.map((pack) => (
        <div
          key={pack}
          role="status"
          className="flex flex-wrap items-center gap-3 rounded-control bg-warn-soft px-3 py-2.5"
        >
          <Icon path={mdiAlertOutline} size={16} className="flex-none text-warn" />
          <p className="m-0 min-w-0 flex-1 text-[12.5px] text-main-100">
            {unconfirmedPack(pack, store.inactiveRules(pack))}
          </p>
          <Button size="sm" disabled={store.rulesBusy !== null} onClick={() => setConfirming(pack)}>
            {PACK_CONFIRM}
          </Button>
        </div>
      ))}
      {store.rules.length === 0 ? (
        <p className="m-0 text-[12.5px] text-main-500">{RULES_EMPTY}</p>
      ) : (
        <ul className="m-0 flex list-none flex-col gap-1 p-0">
          {store.rules.map((rule) => (
            <li
              key={`${rule.pack}:${rule.id}`}
              className="flex items-center gap-3 rounded-control bg-main-800 px-3 py-2"
            >
              <div className="min-w-0 flex-1">
                <p className="m-0 text-[12.5px] wrap-break-word text-main-100">{rule.text}</p>
                <p className="m-0 text-[11.5px] text-main-500">
                  {RULE_EFFECT_LABEL[rule.effect]} · {rule.pack}
                  {rule.createdAt === null ? null : ` · ${ruleCreated(rule.createdAt)}`}
                  {rule.active ? null : <span className="text-warn"> · {RULE_INACTIVE}</span>}
                </p>
              </div>
              <IconButton
                path={mdiClose}
                label={RULE_REMOVE_LABEL}
                size="sm"
                tone="danger"
                disabled={store.rulesBusy !== null}
                onClick={() => void store.removeRule(rule.id)}
              />
            </li>
          ))}
        </ul>
      )}
      {store.rulesError === null ? null : (
        <p className="m-0 text-[12px] text-err">{errorCopy(store.rulesError)}</p>
      )}
      <ConfirmModal
        open={confirming !== null}
        title={PACK_CONFIRM_TITLE}
        content={PACK_CONFIRM_CONSEQUENCE}
        confirmLabel={PACK_CONFIRM}
        onConfirm={() => {
          if (confirming !== null) void store.confirmPack(confirming);
          setConfirming(null);
        }}
        onCancel={() => setConfirming(null)}
      />
    </div>
  );
}

export default observer(RuleList);
