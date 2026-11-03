export interface AssistantFrame {
  readonly mainPath: string;
  readonly headOffsetY: number;
  readonly antennaAngle: number;
}

const BASE_Y = 585;
const DOME_RY = 180;

export function buildAssistantFrame(time: number): AssistantFrame {
  const breathe = 1 + 0.1 * Math.sin(time * 2.4);
  const phase = time * 3.2;
  const pts: string[] = [];
  const push = (x: number, y: number) => pts.push(`${Math.round(x)} ${Math.round(y)}`);
  push(70, BASE_Y);
  push(240, BASE_Y);
  const bumpAmp = 30 * breathe;
  for (let i = 1; i <= 24; i += 1) {
    const x = 240 + (90 * i) / 24;
    push(x, BASE_Y - bumpAmp * Math.sin((Math.PI * (x - 240)) / 90));
  }
  push(370, BASE_Y);
  const ry = DOME_RY * breathe;
  for (let i = 1; i <= 60; i += 1) {
    const t = Math.PI - (Math.PI * i) / 60;
    push(512 + 142 * Math.cos(t), BASE_Y - ry * Math.sin(t));
  }
  for (let i = 1; i <= 80; i += 1) {
    const x = 655 + ((954 - 655) * i) / 80;
    push(x, BASE_Y + 20 * Math.sin(((x - 655) / 140) * 2 * Math.PI - phase));
  }
  return {
    mainPath: `M${pts.join("L")}`,
    headOffsetY: DOME_RY * (1 - breathe),
    antennaAngle: 6 * Math.sin(time * 1.8),
  };
}

export const ASSISTANT_MAIN_STATIC = buildAssistantFrame(0).mainPath;
