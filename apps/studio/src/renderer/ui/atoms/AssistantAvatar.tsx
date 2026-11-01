import { useEffect, useId, useRef } from "react";
import { ASSISTANT_MAIN_STATIC, buildAssistantFrame } from "./assistantGeometry";

// Spiral antenna with a beacon dot, attached to the dome apex.
const ANTENNA_PATH =
  "M512 415L501 414L490 411L480 407L470 403L461 397L453 390L445 383L438 375L433 366L428 357L424 348L422 338L420 328L419 318L420 308L422 299L424 289L428 281L432 272L437 264L443 257L450 251L457 245L464 240L472 237L481 234L489 232L498 230L506 230L515 231L523 233L531 235L538 238L545 242L552 247L558 252L563 258L568 264L572 271L575 278L577 285L578 292L579 299L579 306L578 313L576 320L574 327L571 333L568 339L564 344L559 349L554 353L549 356L543 359L537 361L532 363L526 364L520 364L514 364L508 363L503 361L498 359L493 357L488 354L484 350L481 346L478 342L475 338L473 334L472 329L471 324L470 320L470 315L471 311L472 306L473 302L475 299L478 295L480 292L483 289L486 287L489 285L492 283L496 282L499 281L502 281L506 281L509 281L512 282L512 261L512 239L512 218";

export interface AssistantAvatarProps {
  readonly size?: number;
  readonly live?: boolean;
  readonly className?: string;
}

export default function AssistantAvatar({
  size = 28,
  live = false,
  className = "",
}: AssistantAvatarProps) {
  const id = useId().replace(/[^a-zA-Z0-9]/g, "");
  const mainRef = useRef<SVGPathElement | null>(null);
  const headRef = useRef<SVGGElement | null>(null);

  useEffect(() => {
    const main = mainRef.current;
    const head = headRef.current;
    if (!live || !main || !head) return undefined;
    if (document.documentElement.dataset.motion === "none") return undefined;
    let raf = 0;
    const start = performance.now();
    const tick = (now: number) => {
      const frame = buildAssistantFrame((now - start) / 1000);
      main.setAttribute("d", frame.mainPath);
      head.setAttribute(
        "transform",
        `translate(0 ${frame.headOffsetY.toFixed(1)}) rotate(${frame.antennaAngle.toFixed(2)} 512 415)`,
      );
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => {
      cancelAnimationFrame(raf);
      main.setAttribute("d", ASSISTANT_MAIN_STATIC);
      head.removeAttribute("transform");
    };
  }, [live]);

  return (
    <svg
      viewBox="0 0 1024 1024"
      width={size}
      height={size}
      aria-hidden="true"
      fill="none"
      strokeLinecap="round"
      strokeLinejoin="round"
      className={`flex-none ${live ? "zvs-avatar-live " : ""}${className}`}
    >
      <defs>
        <linearGradient id={`${id}bg`} x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor="#ffc24b" />
          <stop offset="0.35" stopColor="#ff8a3d" />
          <stop offset="0.7" stopColor="#ff553f" />
          <stop offset="1" stopColor="#f43f5e" />
        </linearGradient>
        <radialGradient id={`${id}sheen`} cx="0.26" cy="0.14" r="0.55">
          <stop offset="0" stopColor="#ffffff" stopOpacity="0.26" />
          <stop offset="1" stopColor="#ffffff" stopOpacity="0" />
        </radialGradient>
      </defs>
      <circle cx="512" cy="512" r="512" fill={`url(#${id}bg)`} />
      <circle cx="512" cy="512" r="512" fill={`url(#${id}sheen)`} />
      <g stroke="#fffaf3">
        <circle cx="448" cy="548" r="30" fill="#fffaf3" stroke="none" className="zvs-eye" />
        <circle cx="576" cy="548" r="30" fill="#fffaf3" stroke="none" className="zvs-eye" />
        <path ref={mainRef} d={ASSISTANT_MAIN_STATIC} strokeWidth={46} />
        <g ref={headRef}>
          <path d={ANTENNA_PATH} strokeWidth={42} />
          <circle cx="512" cy="192" r="26" fill="#fffaf3" stroke="none" />
        </g>
      </g>
    </svg>
  );
}
