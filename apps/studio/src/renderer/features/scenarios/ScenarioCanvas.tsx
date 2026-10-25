import {
  mdiClose,
  mdiFitToScreenOutline,
  mdiFormatAlignLeft,
  mdiMagnifyMinusOutline,
  mdiMagnifyPlusOutline,
  mdiPlay,
  mdiPlus,
  mdiRedo,
  mdiStop,
  mdiUndo,
} from "@mdi/js";
import {
  Background,
  BackgroundVariant,
  ReactFlow,
  ReactFlowProvider,
  useReactFlow,
  useViewport,
  type Connection,
  type EdgeChange,
  type NodeChange,
} from "@xyflow/react";
import "@xyflow/react/dist/base.css";
import { observer } from "mobx-react-lite";
import { useCallback, useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import Button from "../../ui/atoms/buttons/Button";
import IconButton from "../../ui/atoms/buttons/IconButton";
import Icon from "../../ui/atoms/Icon";
import {
  flowEdges,
  flowNodes,
  IN_HANDLE,
  OUT_HANDLE,
  type ScenarioFlowEdge,
  type ScenarioFlowNode,
} from "./flowModel";
import ApprovalBar from "./ApprovalBar";
import { CanvasActionsContext } from "./canvasActions";
import { nodeSize } from "./layout";
import NodePalette from "./NodePalette";
import type ScenarioEditor from "./ScenarioEditor";
import ScenarioEdgeView from "./ScenarioEdgeView";
import ScenarioNodeView from "./ScenarioNodeView";
import { canvasCommand, isEditableTarget } from "./shortcuts";

const NODE_TYPES = { scenario: ScenarioNodeView };
const EDGE_TYPES = { scenario: ScenarioEdgeView };
const PAN_BUTTONS = [1];
const FIT_OPTIONS = { padding: 0.25, maxZoom: 1, duration: 180 };
const TEST_BLOCKED = "Запуск недоступен, пока в сценарии есть ошибки";

const FLOW_STYLE = {
  "--xy-selection-background-color": "var(--color-accent-soft)",
  "--xy-selection-border": "1px solid var(--color-accent-line)",
  "--xy-connectionline-stroke": "var(--color-accent-dark)",
  "--xy-connectionline-stroke-width": "1.6",
  "--xy-background-color": "var(--color-main-900)",
} as CSSProperties;

type Size = { width: number; height: number };

interface ToolProps {
  readonly label: string;
  readonly icon: string;
  readonly disabled?: boolean;
  readonly onClick: () => void;
}

function Tool({ label, icon, disabled = false, onClick }: ToolProps) {
  return (
    <IconButton
      path={icon}
      label={label}
      iconSize={16}
      disabled={disabled}
      onClick={onClick}
      className="size-7.5 rounded-[7px] text-main-300"
    />
  );
}

const Toolbar = observer(function Toolbar({ editor }: { readonly editor: ScenarioEditor }) {
  const flow = useReactFlow();
  const { zoom } = useViewport();
  return (
    <div className="absolute bottom-4.5 left-4.5 z-10 flex gap-1.5 rounded-[10px] border border-line bg-main-800 p-1.25 shadow-card">
      <Tool
        label="Показать весь сценарий"
        icon={mdiFitToScreenOutline}
        onClick={() => void flow.fitView(FIT_OPTIONS)}
      />
      <Tool
        label="Приблизить"
        icon={mdiMagnifyPlusOutline}
        onClick={() => void flow.zoomIn({ duration: 120 })}
      />
      <Tool
        label="Отдалить"
        icon={mdiMagnifyMinusOutline}
        onClick={() => void flow.zoomOut({ duration: 120 })}
      />
      <Tool label="Отменить" icon={mdiUndo} disabled={!editor.canUndo} onClick={editor.undo} />
      <Tool label="Повторить" icon={mdiRedo} disabled={!editor.canRedo} onClick={editor.redo} />
      <Tool
        label="Упорядочить"
        icon={mdiFormatAlignLeft}
        disabled={editor.graph.nodes.length === 0}
        onClick={() => {
          editor.tidy();
          window.requestAnimationFrame(() => void flow.fitView(FIT_OPTIONS));
        }}
      />
      <span className="mx-0.5 my-1 w-px bg-line" />
      <span className="flex h-7.5 items-center px-2 font-mono text-[11px] text-main-400">
        {`${String(Math.round(zoom * 100))}%`}
      </span>
    </div>
  );
});

const CanvasInner = observer(function CanvasInner({ editor }: { readonly editor: ScenarioEditor }) {
  const flow = useReactFlow();
  const frame = useRef<HTMLDivElement | null>(null);
  const actions = useMemo(
    () => ({ duplicate: editor.duplicateNode, remove: editor.deleteNode }),
    [editor],
  );
  const [measured, setMeasured] = useState<Record<string, Size>>({});

  const nodes = flowNodes(editor).map((node) => {
    const size = measured[node.id];
    return size === undefined ? node : { ...node, measured: size };
  });
  const edges = flowEdges(editor);

  const onNodesChange = useCallback(
    (changes: NodeChange<ScenarioFlowNode>[]) => {
      const sizes: Record<string, Size> = {};
      const dragging: Record<string, { x: number; y: number }> = {};
      const dropped: Record<string, { x: number; y: number }> = {};
      const selected = new Set(editor.selection.nodes);
      let selection = false;
      for (const change of changes) {
        if (change.type === "dimensions" && change.dimensions !== undefined) {
          sizes[change.id] = change.dimensions;
        } else if (change.type === "position" && change.position !== undefined) {
          if (change.dragging === true) dragging[change.id] = change.position;
          else dropped[change.id] = change.position;
        } else if (change.type === "select") {
          selection = true;
          if (change.selected) selected.add(change.id);
          else selected.delete(change.id);
        }
      }
      if (Object.keys(sizes).length > 0) setMeasured((previous) => ({ ...previous, ...sizes }));
      if (Object.keys(dragging).length > 0) editor.dragNodes(dragging);
      if (Object.keys(dropped).length > 0) editor.endDrag(dropped);
      if (selection) editor.setSelection({ nodes: [...selected], links: editor.selection.links });
    },
    [editor],
  );

  const onEdgesChange = useCallback(
    (changes: EdgeChange<ScenarioFlowEdge>[]) => {
      const selected = new Set(editor.selection.links);
      let selection = false;
      for (const change of changes) {
        if (change.type !== "select") continue;
        selection = true;
        if (change.selected) selected.add(change.id);
        else selected.delete(change.id);
      }
      if (selection) editor.setSelection({ nodes: editor.selection.nodes, links: [...selected] });
    },
    [editor],
  );

  const onConnect = useCallback(
    (connection: Connection) => {
      editor.connect(
        connection.source,
        connection.sourceHandle ?? null,
        connection.target,
        connection.targetHandle ?? null,
      );
    },
    [editor],
  );

  const onReconnect = useCallback(
    (edge: ScenarioFlowEdge, connection: Connection) => {
      if (edge.data?.kind !== "main") return;
      if (connection.sourceHandle !== OUT_HANDLE || connection.targetHandle !== IN_HANDLE) return;
      editor.reroute(edge.id, connection.source, connection.target);
    },
    [editor],
  );

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const command = canvasCommand(event);
      if (command === null) return;
      if (command === "save") {
        event.preventDefault();
        if (document.activeElement instanceof HTMLElement) document.activeElement.blur();
        if (editor.dirty) void editor.save();
        return;
      }
      if (isEditableTarget(event.target)) return;
      event.preventDefault();
      if (command === "delete") editor.deleteSelection();
      else if (command === "duplicate") editor.duplicateSelection();
      else if (command === "selectAll") editor.selectAll();
      else if (command === "deselect") editor.clearSelection();
      else if (command === "undo") editor.undo();
      else editor.redo();
    };
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
    };
  }, [editor]);

  const addStep = useCallback(
    (type: string) => {
      const rect = frame.current?.getBoundingClientRect();
      const centre =
        rect === undefined
          ? { x: 0, y: 0 }
          : flow.screenToFlowPosition({
              x: rect.left + rect.width / 2,
              y: rect.top + rect.height / 2,
            });
      const size = nodeSize(editor.catalogue.get(type));
      editor.addStep(type, {
        x: Math.round(centre.x - size.width / 2),
        y: Math.round(centre.y - size.height / 2),
      });
    },
    [editor, flow],
  );

  return (
    <CanvasActionsContext.Provider value={actions}>
      <div ref={frame} className="relative min-h-0 min-w-0 flex-1 bg-main-900">
        <ReactFlow<ScenarioFlowNode, ScenarioFlowEdge>
          nodes={nodes}
          edges={edges}
          nodeTypes={NODE_TYPES}
          edgeTypes={EDGE_TYPES}
          onNodesChange={onNodesChange}
          onEdgesChange={onEdgesChange}
          onConnect={onConnect}
          onReconnect={onReconnect}
          onPaneClick={() => {
            editor.closePalette();
          }}
          edgesReconnectable
          deleteKeyCode={null}
          selectionKeyCode={null}
          multiSelectionKeyCode={["Shift", "Meta", "Control"]}
          panActivationKeyCode="Space"
          panOnDrag={PAN_BUTTONS}
          selectionOnDrag
          zoomOnDoubleClick={false}
          minZoom={0.25}
          maxZoom={2}
          connectionRadius={24}
          fitView
          fitViewOptions={FIT_OPTIONS}
          proOptions={{ hideAttribution: true }}
          style={FLOW_STYLE}
        >
          <Background
            id="scenario-dots"
            variant={BackgroundVariant.Dots}
            gap={22}
            size={1}
            color="var(--color-main-700)"
          />
        </ReactFlow>

        {editor.graph.nodes.length === 0 ? (
          <div className="pointer-events-none absolute inset-0 flex items-center justify-center">
            <div className="pointer-events-auto flex max-w-80 flex-col items-center gap-3 text-center">
              <span className="text-[14px] font-semibold text-main-50">Сценарий пуст</span>
              <span className="text-[12.5px] leading-relaxed text-main-400">
                Сценарий начинается с триггера. Добавьте его, затем шаги, которые он запускает.
              </span>
              <Button tone="primary" onClick={editor.openPalette}>
                <Icon path={mdiPlus} size={15} />
                Добавить шаг
              </Button>
            </div>
          </div>
        ) : null}

        <ApprovalBar editor={editor} />

        {editor.refusal === null || editor.execution.approval !== null ? null : (
          <div
            role="alert"
            className="absolute top-4 left-1/2 z-20 flex max-w-[min(560px,calc(100%-120px))] -translate-x-1/2 items-center gap-2.5 rounded-control border border-err-border bg-err-soft px-3 py-2.25 text-[12px] text-err shadow-card"
          >
            <span className="min-w-0 flex-1">{editor.refusal}</span>
            <IconButton
              path={mdiClose}
              label="Скрыть"
              tooltip="bottom-center"
              onClick={editor.dismissRefusal}
              className="size-5 text-err/80 hover:bg-transparent hover:text-err"
            />
          </div>
        )}

        <div className="absolute top-4 right-4.5 z-10">
          <IconButton
            path={mdiPlus}
            label="Добавить шаг"
            tooltip="left-center"
            iconSize={18}
            onClick={editor.togglePalette}
            className={`size-8.5 rounded-[9px] border bg-main-800 text-main-200 shadow-card ${
              editor.paletteOpen ? "border-accent-dark" : "border-line hover:border-line-strong"
            }`}
          />
        </div>

        {editor.paletteOpen ? <NodePalette editor={editor} onPick={addStep} /> : null}

        <Toolbar editor={editor} />

        {editor.graph.nodes.length === 0 ? null : (
          <div className="absolute bottom-4.5 left-1/2 z-10 flex -translate-x-1/2 gap-2">
            {editor.execution.running ? (
              <Button
                tone="primary"
                disabled={editor.execution.cancelling || editor.execution.view === null}
                onClick={() => void editor.execution.cancel()}
              >
                <Icon path={mdiStop} size={15} />
                Остановить
              </Button>
            ) : (
              <Button
                tone="primary"
                disabled={editor.issues.length > 0}
                title={editor.issues.length > 0 ? TEST_BLOCKED : undefined}
                onClick={() => void editor.testRun()}
              >
                <Icon path={mdiPlay} size={15} />
                Тестовый запуск
              </Button>
            )}
            <Button tone="secondary" onClick={editor.openPalette}>
              <Icon path={mdiPlus} size={15} />
              Добавить шаг
            </Button>
          </div>
        )}
      </div>
    </CanvasActionsContext.Provider>
  );
});

export default function ScenarioCanvas({ editor }: { readonly editor: ScenarioEditor }) {
  return (
    <ReactFlowProvider>
      <CanvasInner editor={editor} />
    </ReactFlowProvider>
  );
}
