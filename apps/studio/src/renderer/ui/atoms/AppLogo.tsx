import { useId } from "react";

export interface AppLogoProps {
  readonly size?: number;
  readonly className?: string;
}

const MAIN_PATH =
  "M70 520L180 520L184 517L188 514L193 510L197 507L201 505L205 502L209 500L214 498L218 496L222 495L226 494L230 494L235 494L239 494L243 495L247 496L251 498L256 500L260 502L264 505L268 507L272 510L277 514L281 517L285 520L288 531L292 542L295 553L298 564L302 575L305 585L309 596L312 607L315 618L319 629L322 640L327 626L333 613L338 599L344 585L349 572L355 558L360 544L365 531L371 517L376 503L382 489L387 476L392 462L398 448L403 435L409 421L414 407L420 394L425 380L430 394L436 407L441 421L447 435L452 448L458 462L463 476L468 489L474 503L479 517L485 531L490 544L495 558L501 572L506 585L512 599L517 613L523 626L528 640L545 640L562 640L578 640L595 640L612 640L615 629L619 618L622 607L626 596L629 585L633 575L636 564L640 553L643 542L647 531L650 520L653 523L657 526L660 529L664 532L667 534L670 537L674 539L677 540L681 541L684 542L688 542L691 542L694 541L698 540L701 538L705 537L708 534L711 532L715 529L718 526L722 523L725 520L729 517L732 514L735 511L739 508L742 505L746 503L749 501L752 500L756 499L759 498L763 498L766 498L770 499L773 500L776 502L780 504L783 506L787 508L790 511L793 514L797 517L800 520L804 523L807 526L811 529L814 532L817 535L821 537L824 539L828 540L831 541L834 542L838 542L841 542L845 541L848 540L852 538L855 536L858 534L862 532L865 529L869 526L872 523L875 520L879 516L882 513L886 510L889 508L893 505L896 503L899 501L903 500L906 499L910 498L913 498L916 498L920 499L923 500L927 502L930 504L934 506L937 509L940 511L944 514L947 517L951 521L954 524";
const STEM_PATH =
  "M612 452L612 473L612 494L612 515L612 536L612 556L612 577L612 598L612 619L612 640";
const CROSS_PATH = "M362 545L488 545";
const MARK_PATH = `${MAIN_PATH}${STEM_PATH}${CROSS_PATH}`;

export default function AppLogo({ size = 32, className = "" }: AppLogoProps) {
  const id = useId().replace(/[^a-zA-Z0-9]/g, "");
  return (
    <svg
      viewBox="0 0 1024 1024"
      width={size}
      height={size}
      aria-hidden="true"
      className={`flex-none ${className}`}
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
      <rect x="26" y="26" width="972" height="972" rx="230" fill={`url(#${id}bg)`} />
      <rect x="26" y="26" width="972" height="972" rx="230" fill={`url(#${id}sheen)`} />
      <g fill="none" stroke="#fff9f2" strokeLinecap="round" strokeLinejoin="round">
        <g opacity="0.3">
          <path d={MARK_PATH} strokeWidth={58} />
        </g>
        <path d={MARK_PATH} strokeWidth={33} />
      </g>
      <circle cx="612" cy="404" r="38" fill="#fff9f2" opacity="0.3" />
      <circle cx="612" cy="404" r="24" fill="#fff9f2" />
      <text
        x="520"
        y="812"
        textAnchor="middle"
        fontFamily="inherit"
        fontSize="54"
        fontWeight="600"
        letterSpacing="16"
        fill="#fff6ec"
        opacity="0.95"
      >
        STUDIO
      </text>
    </svg>
  );
}
