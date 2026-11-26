/**
 * App icons — Tabler Icons only (https://tabler.io/icons).
 * Stable `Icon*` names for call sites. No other icon libraries / local SVG packs.
 */

import type { ComponentType } from "react";
import {
  IconActivity as TbActivity,
  IconAdjustmentsHorizontal as TbAdjustmentsHorizontal,
  IconAlertTriangle as TbAlertTriangle,
  IconArrowsMaximize as TbArrowsMaximize,
  IconArchive as TbArchive,
  IconArrowBackUp as TbArrowBackUp,
  IconArrowLeft as TbArrowLeft,
  IconArrowUp as TbArrowUp,
  IconCircleArrowUp as TbCircleArrowUp,
  IconArrowsMinimize as TbArrowsMinimize,
  IconFocus2 as TbFocus2,
  IconBlockquote as TbBlockquote,
  IconBell as TbBell,
  IconBellOff as TbBellOff,
  IconBold as TbBold,
  IconBolt as TbBolt,
  IconBrandGithub as TbBrandGithub,
  IconGitBranch as TbGitBranch,
  IconGitCommit as TbGitCommit,
  IconBox as TbBox,
  IconBoxMultiple as TbBoxMultiple,
  IconBrush as TbBrush,
  IconCamera as TbCamera,
  IconCalendarTime as TbCalendarTime,
  IconCheck as TbCheck,
  IconClearAll as TbClearAll,
  IconClipboardList as TbClipboardList,
  IconClock as TbClock,
  IconCode as TbCode,
  IconChevronDown as TbChevronDown,
  IconChevronLeft as TbChevronLeft,
  IconChevronRight as TbChevronRight,
  IconChevronUp as TbChevronUp,
  IconChevronsLeft as TbChevronsLeft,
  IconBulb as TbBulb,
  IconCircle as TbCircle,
  IconCircleDashed as TbCircleDashed,
  IconClick as TbClick,
  IconCopy as TbCopy,
  IconGridDots as TbGridDots,
  IconGripVertical as TbGripVertical,
  IconDeviceDesktop as TbDeviceDesktop,
  IconDeviceMobile as TbDeviceMobile,
  IconDots as TbDots,
  IconCrop as TbCrop,
  IconEdit as TbEdit,
  IconEye as TbEye,
  IconH1 as TbH1,
  IconH2 as TbH2,
  IconH3 as TbH3,
  IconItalic as TbItalic,
  IconFileDiff as TbFileDiff,
  IconFileText as TbFileText,
  IconFiles as TbFiles,
  IconFirstAidKit as TbFirstAidKit,
  IconFolder as TbFolder,
  IconFolderPlus as TbFolderPlus,
  IconHandStop as TbHandStop,
  IconHeart as TbHeart,
  IconHelp as TbHelp,
  IconHome as TbHome,
  IconHexagon as TbHexagon,
  IconInfoCircle as TbInfoCircle,
  IconKeyboard as TbKeyboard,
  IconKey as TbKey,
  IconLanguage as TbLanguage,
  IconExternalLink as TbExternalLink,
  IconLayoutSidebar as TbLayoutSidebar,
  IconLayoutSidebarRight as TbLayoutSidebarRight,
  IconLink as TbLink,
  IconList as TbList,
  IconListCheck as TbListCheck,
  IconListNumbers as TbListNumbers,
  IconListTree as TbListTree,
  IconMarkdown as TbMarkdown,
  IconMenu2 as TbMenu2,
  IconMessage as TbMessage,
  IconMicrophone as TbMicrophone,
  IconHeadphones as TbHeadphones,
  IconMinus as TbMinus,
  IconMoon as TbMoon,
  IconNotes as TbNotes,
  IconPaperclip as TbPaperclip,
  IconPencil as TbPencil,
  IconPinned as TbPinned,
  IconPinnedOff as TbPinnedOff,
  IconPlayerStop as TbPlayerStop,
  IconPlayerPlay as TbPlayerPlay,
  IconPlug as TbPlug,
  IconPlus as TbPlus,
  IconPuzzle as TbPuzzle,
  IconRefresh as TbRefresh,
  IconRobot as TbRobot,
  IconSearch as TbSearch,
  IconSend as TbSend,
  IconSeparator as TbSeparator,
  IconSettings as TbSettings,
  IconShield as TbShield,
  IconShieldCheck as TbShieldCheck,
  IconSparkles as TbSparkles,
  IconSquare as TbSquare,
  IconStack2 as TbStack2,
  IconStrikethrough as TbStrikethrough,
  IconSun as TbSun,
  IconSwitchHorizontal as TbSwitchHorizontal,
  IconTarget as TbTarget,
  IconTerminal2 as TbTerminal2,
  IconThumbDown as TbThumbDown,
  IconThumbUp as TbThumbUp,
  IconTool as TbTool,
  IconTrash as TbTrash,
  IconUpload as TbUpload,
  IconUser as TbUser,
  IconPhoto as TbPhoto,
  IconPhotoSearch as TbPhotoSearch,
  IconMovie as TbMovie,
  IconWand as TbWand,
  IconCloud as TbCloud,
  IconWorld as TbWorld,
  IconX as TbX,
} from "@tabler/icons-react";

export type IconProps = {
  size?: number;
  title?: string;
  className?: string;
  stroke?: number;
  /** @deprecated No-op; call-site compatibility with previous icon APIs. */
  animated?: boolean;
  /** @deprecated No-op; call-site compatibility with Phosphor weight. */
  weight?: string;
};

type TbIcon = ComponentType<{
  size?: number | string;
  stroke?: number;
  color?: string;
  className?: string;
  "aria-hidden"?: boolean | "true" | "false";
}>;

function wrap(Tb: TbIcon, defaults?: { stroke?: number; className?: string }) {
  function TablerAppIcon({
    size = 18,
    title,
    stroke = defaults?.stroke ?? 1.75,
    className = "",
    animated: _a,
    weight: _w,
  }: IconProps) {
    const classes = ["g-icon", defaults?.className, className]
      .filter(Boolean)
      .join(" ");
    return (
      <span
        className={classes}
        style={{
          display: "inline-flex",
          width: size,
          height: size,
          lineHeight: 0,
          color: "currentColor",
          flexShrink: 0,
          alignItems: "center",
          justifyContent: "center",
        }}
        role={title ? "img" : undefined}
        aria-hidden={title ? undefined : true}
        aria-label={title}
        title={title}
      >
        <Tb size={size} stroke={stroke} color="currentColor" aria-hidden />
      </span>
    );
  }
  return TablerAppIcon;
}

/**
 * Atlas A silhouette (currentColor) — follows theme via CSS color.
 * Dark: light glyph; light: dark glyph (no invert filter needed).
 */
export function IconGrokMark({
  size = 22,
  title = "Atlas",
  className = "",
}: IconProps) {
  const classes = ["g-icon", "g-icon--grok-mark", className]
    .filter(Boolean)
    .join(" ");
  return (
    <span
      className={classes}
      style={{
        display: "inline-flex",
        width: size,
        height: size,
        lineHeight: 0,
        color: "currentColor",
        flexShrink: 0,
        alignItems: "center",
        justifyContent: "center",
      }}
      role={title ? "img" : undefined}
      aria-hidden={title ? undefined : true}
      aria-label={title}
      title={title}
    >
      <svg
        width={size}
        height={size}
        viewBox="0 0 1024 1024"
        fill="none"
        xmlns="http://www.w3.org/2000/svg"
        aria-hidden
      >
        <path
          d="M512,62L503,74L496,95L486,106L487,112L481,117L481,124L473,133L472,141L463,153L464,157L459,160L460,165L454,170L452,181L446,186L427,223L428,227L420,236L415,246L416,250L412,253L413,257L405,266L406,270L400,276L401,281L395,286L394,294L388,300L389,304L373,328L372,338L359,356L349,376L350,380L339,396L337,406L332,409L333,413L327,419L325,429L320,432L321,436L313,446L311,457L305,462L298,476L299,480L293,486L294,490L276,519L276,526L267,536L265,546L261,549L255,566L237,597L237,603L227,616L228,621L215,639L216,644L205,660L206,664L200,669L190,688L189,697L183,702L184,707L176,716L177,721L169,730L165,743L161,746L162,750L154,760L155,765L147,773L145,784L137,793L135,804L130,807L128,818L120,827L121,832L115,837L116,840L110,847L111,852L103,861L103,868L98,878L93,881L94,886L69,929L66,942L59,949L57,961L95,926L99,926L101,931L87,948L195,871L209,865L219,854L226,852L228,847L232,847L251,828L255,828L254,824L267,809L276,804L305,771L397,686L635,686L680,711L683,716L687,715L717,731L697,715L698,712L691,710L688,701L692,698L733,735L729,740L722,740L722,760L764,807L763,810L774,818L792,842L803,851L805,857L823,869L828,869L830,874L844,884L851,885L857,893L868,897L878,908L881,907L922,938L940,947L935,941L939,934L949,940L963,957L969,960L969,952L941,902L942,897L937,894L878,778L879,773L871,764L817,658L818,654L810,644L811,640L807,637L803,624L795,614L796,610L783,591L784,587L780,584L782,580L769,564L769,556L764,554L764,547L756,538L754,526L747,521L740,502L729,485L730,482L725,478L723,468L718,464L718,458L713,454L705,438L706,434L693,415L673,375L674,371L666,361L664,351L656,342L657,337L651,332L652,327L646,321L647,317L641,312L642,307L632,295L633,290L624,278L625,273L617,265L618,261L612,255L608,241L603,238L595,222L596,217L581,195L571,175L572,171L566,165L564,154L559,152L560,147L552,138L552,131L547,128L540,109L536,106L537,101L532,98L533,95L524,82L525,77L518,71L518,64ZM515,288L542,336L544,346L550,352L549,356L554,359L554,365L563,383L569,388L569,396L577,404L578,413L584,418L588,432L593,435L655,556L654,561L659,564L671,589L689,615L685,618L673,610L668,610L666,605L657,602L649,593L641,588L635,588L629,581L618,577L613,571L608,571L572,544L569,545L564,539L561,540L552,534L547,527L537,526L596,591L595,598L433,598L431,591L462,557L466,557L465,553L485,533L489,526L479,527L459,544L454,544L417,571L399,580L395,578L396,582L351,612L341,622L336,622L335,615L367,571L369,560L376,546L384,538L383,533L389,528L388,524L401,498L409,492L410,480L430,449L434,434L440,429L439,424L444,414L452,406L451,401L508,291Z"
          fill="currentColor"
          fillRule="evenodd"
          id="mark"
        />
      </svg>
    </span>
  );
}

export const IconCollapse = wrap(TbChevronsLeft);
export const IconSearch = wrap(TbSearch);
/** New chat / compose — Tabler Edit (pencil writing on paper). */
export const IconNewChat = wrap(TbEdit);
export const IconEdit = wrap(TbEdit);
/** Markdown / TipTap format toolbar */
export const IconBold = wrap(TbBold);
export const IconItalic = wrap(TbItalic);
export const IconStrikethrough = wrap(TbStrikethrough);
export const IconCode = wrap(TbCode);
export const IconH1 = wrap(TbH1);
export const IconH2 = wrap(TbH2);
export const IconH3 = wrap(TbH3);
export const IconListNumbers = wrap(TbListNumbers);
export const IconBlockquote = wrap(TbBlockquote);
export const IconSeparator = wrap(TbSeparator);
/** Wallpaper focus / crop frame editor. */
export const IconCrop = wrap(TbCrop);
export const IconNotes = wrap(TbNotes);
export const IconImagine = wrap(TbWand);
export const IconHeart = wrap(TbHeart);
export const IconVideo = wrap(TbMovie);
export const IconPlay = wrap(TbPlayerPlay);
export const IconAutomations = wrap(TbBolt);
/** Scheduled / “已安排” nav — calendar clock. */
export const IconScheduled = wrap(TbCalendarTime);
export const IconClock = wrap(TbClock);
export const IconSkills = wrap(TbTool);
/** Lifecycle hooks (PreToolUse / SessionStart, …). */
export const IconHooks = wrap(TbBolt);
export const IconChevronDown = wrap(TbChevronDown);
/** Space / view switch — distinct from L1 expand chevrons. */
export const IconSwitch = wrap(TbSwitchHorizontal);
export const IconChevronLeft = wrap(TbChevronLeft);
export const IconChevronRight = wrap(TbChevronRight);
export const IconChevronUp = wrap(TbChevronUp);
export const IconFolderPlus = wrap(TbFolderPlus);
export const IconPlus = wrap(TbPlus);
export const IconMore = wrap(TbDots);
export const IconFolder = wrap(TbFolder);
export const IconRename = wrap(TbPencil);
export const IconLink = wrap(TbLink);
export const IconTrash = wrap(TbTrash, { className: "g-icon--danger" });
export const IconPaperclip = wrap(TbPaperclip);
export const IconAttach = wrap(TbPaperclip);
export const IconClose = wrap(TbX);
/** Close / clear every item in a strip (bottom terminal close-all). */
export const IconClearAll = wrap(TbClearAll);
export const IconSend = wrap(TbSend);
/** Up arrow — composer send button glyph. */
export const IconArrowUp = wrap(TbArrowUp);
/** Circle arrow up — sidebar / About app update affordance. */
export const IconCircleArrowUp = wrap(TbCircleArrowUp);
export const IconQueue = wrap(TbStack2);
export const IconMic = wrap(TbMicrophone);
export const IconLiveVoice = wrap(TbHeadphones);
export const IconPanel = wrap(TbLayoutSidebar);
/** Hamburger / phone session drawer toggle. */
export const IconMenu = wrap(TbMenu2);
/** Right files / context pane (Codex-style top bar). */
export const IconPanelRight = wrap(TbLayoutSidebarRight);
/** Environment info menu (env ··|· knobs). */
export const IconEnv = wrap(TbAdjustmentsHorizontal);
export const IconBrandGithub = wrap(TbBrandGithub);
export const IconGitCommit = wrap(TbGitCommit);
/** Expand side workbench into chat area (四角外扩). */
export const IconSideExpand = wrap(TbArrowsMaximize);
/** Floating composer toggle when side is expanded. */
export const IconFloatComposer = wrap(TbMessage);
/** Drag handle for the floating composer card. */
export const IconDragHandle = wrap(TbGripVertical);
/** Terminal tab. */
export const IconTerminal = wrap(TbTerminal2);
/** Open project in Finder / external app. */
export const IconExternalLink = wrap(TbExternalLink);
export const IconList = wrap(TbList);
/** Sidebar multi-select / checklist mode. */
export const IconListCheck = wrap(TbListCheck);
export const IconInstructions = wrap(TbFileText);
export const IconSettings = wrap(TbSettings);
export const IconHexagon = wrap(TbHexagon);

/** Settings / slash pet — hex body with two short vertical eyes. */
export function IconPet({
  size = 18,
  title,
  stroke = 1.75,
  className = "",
}: IconProps) {
  const classes = ["g-icon", className].filter(Boolean).join(" ");
  return (
    <span
      className={classes}
      style={{
        display: "inline-flex",
        width: size,
        height: size,
        lineHeight: 0,
        color: "currentColor",
        flexShrink: 0,
        alignItems: "center",
        justifyContent: "center",
      }}
      role={title ? "img" : undefined}
      aria-hidden={title ? undefined : true}
      aria-label={title}
      title={title}
    >
      <svg
        width={size}
        height={size}
        viewBox="0 0 24 24"
        fill="none"
        xmlns="http://www.w3.org/2000/svg"
        aria-hidden
      >
        <path
          d="M19.875 6.27c.7.398 1.13 1.143 1.125 1.948v7.284c0 .809-.443 1.555-1.158 1.948l-6.75 4.27a2.269 2.269 0 0 1-2.184 0l-6.75-4.27a2.225 2.225 0 0 1-1.158-1.948v-7.285c0-.809.443-1.554 1.158-1.947l6.75-3.98a2.33 2.33 0 0 1 2.25 0l6.75 3.98z"
          stroke="currentColor"
          strokeWidth={stroke}
          strokeLinecap="round"
          strokeLinejoin="round"
        />
        <path
          d="M9.6 10.2v2.7M14.4 10.2v2.7"
          stroke="currentColor"
          strokeWidth={stroke}
          strokeLinecap="round"
        />
      </svg>
    </span>
  );
}
export const IconDoctor = wrap(TbFirstAidKit);
export const IconThemeSun = wrap(TbSun);
export const IconThemeMoon = wrap(TbMoon);
/** Composer stop action — outline player stop. */
export const IconStop = wrap(TbPlayerStop);
export const IconHistory = wrap(TbRefresh);
/** Session rewind / undo conversation tail. */
export const IconRewind = wrap(TbArrowBackUp);
/** Session fork / branch. */
export const IconFork = wrap(TbGitBranch);
/** Git branch indicator (composer context bar). */
export const IconGitBranch = wrap(TbGitBranch);
/** Local machine / desktop workspace. */
export const IconDeviceDesktop = wrap(TbDeviceDesktop);
export const IconUpload = wrap(TbUpload);
export const IconFiles = wrap(TbFiles);
/** Session changes / diff panel (resource viewer). */
export const IconFileDiff = wrap(TbFileDiff);
/** File tree panel toggle (resource viewer). */
export const IconListTree = wrap(TbListTree);
export const IconFileUp = wrap(TbUpload);
export const IconThumbsUp = wrap(TbThumbUp);
export const IconThumbsDown = wrap(TbThumbDown);
export const IconRefresh = wrap(TbRefresh);
export const IconCopy = wrap(TbCopy);
/** View / preview (review tree row). */
export const IconEye = wrap(TbEye);
/** Connect phone / remote mirror. */
export const IconDeviceMobile = wrap(TbDeviceMobile);
export const IconExportMd = wrap(TbMarkdown);
/** Conversation share-card / export as image. */
export const IconExportImage = wrap(TbPhoto);
export const IconCamera = wrap(TbCamera);
export const IconPhotoSearch = wrap(TbPhotoSearch);
export const IconArchive = wrap(TbArchive);
export const IconChat = wrap(TbMessage);
export const IconFileText = wrap(TbFileText);
export const IconBolt = wrap(TbBolt);
export const IconMinimize = wrap(TbMinus);
export const IconMaximize = wrap(TbSquare);
/** Caption restore (overlapping squares). */
export const IconRestore = wrap(TbBoxMultiple);
export const IconPlan = wrap(TbList);
export const IconPin = wrap(TbPinned);
export const IconPinOff = wrap(TbPinnedOff);
/** Per-session desktop notification mute (sidebar / context menu). */
export const IconBell = wrap(TbBell);
export const IconBellOff = wrap(TbBellOff);
export const IconHandStop = wrap(TbHandStop);
export const IconShield = wrap(TbShield);
export const IconShieldCheck = wrap(TbShieldCheck);
export const IconAlertTriangle = wrap(TbAlertTriangle);
export const IconCheck = wrap(TbCheck);
export const IconRobot = wrap(TbRobot);
export const IconArrowLeft = wrap(TbArrowLeft);
export const IconUser = wrap(TbUser);
export const IconAppearance = wrap(TbBrush);
export const IconLanguage = wrap(TbLanguage);
export const IconInfo = wrap(TbInfoCircle);
/** Help / “?” tip trigger next to settings labels. */
export const IconHelp = wrap(TbHelp);
/** Unbound chats / Default workspace. */
export const IconHome = wrap(TbHome, { className: "g-icon--home" });
export const IconKeyboard = wrap(TbKeyboard);
export const IconKey = wrap(TbKey);
/** Slash palette / goal mode */
export const IconTarget = wrap(TbTarget);
/** Side-browser Design Mode — click to inspect. */
export const IconClick = wrap(TbClick);
export const IconClipboardList = wrap(TbClipboardList);
export const IconArrowsMinimize = wrap(TbArrowsMinimize);
/** Zen mode — hide side panes and focus the chat. */
export const IconZen = wrap(TbFocus2);

/**
 * Two chevrons facing each other (∨ above ∧) — collapse all project folders.
 * Glyph is slightly inset with a clearer mid gap; stroke stays Tabler 1.75
 * so weight matches IconPlus at the same box size.
 */
export function IconArrowsVerticalCollapse({
  size = 15,
  title,
  stroke = 1.75,
  className = "",
}: IconProps) {
  const classes = ["g-icon", className].filter(Boolean).join(" ");
  return (
    <span
      className={classes}
      style={{
        display: "inline-flex",
        width: size,
        height: size,
        lineHeight: 0,
        color: "currentColor",
        flexShrink: 0,
        alignItems: "center",
        justifyContent: "center",
      }}
      role={title ? "img" : undefined}
      aria-hidden={title ? undefined : true}
      aria-label={title}
      title={title}
    >
      <svg
        width={size}
        height={size}
        viewBox="0 0 24 24"
        fill="none"
        xmlns="http://www.w3.org/2000/svg"
        aria-hidden
      >
        {/* Upper chevron: ∨ — smaller, higher */}
        <path
          d="M8.5 7L12 10.25L15.5 7"
          stroke="currentColor"
          strokeWidth={stroke}
          strokeLinecap="round"
          strokeLinejoin="round"
        />
        {/* Lower chevron: ∧ — smaller, lower (wider mid gap) */}
        <path
          d="M8.5 17L12 13.75L15.5 17"
          stroke="currentColor"
          strokeWidth={stroke}
          strokeLinecap="round"
          strokeLinejoin="round"
        />
      </svg>
    </span>
  );
}
export const IconCircleDashed = wrap(TbCircleDashed);
/** Hollow circle — Grok timeline intermediate tool steps. */
export const IconCircle = wrap(TbCircle);
/** Lightbulb — Grok timeline thinking rows. */
export const IconBulb = wrap(TbBulb);
/** Globe — Grok timeline "Browsed …" rows. */
export const IconWorld = wrap(TbWorld);
/** Cloud — Atlas Cloud nav. */
export const IconCloud = wrap(TbCloud);
/** 6-dot grid — Grok "Working for Ns" live indicator. */
export const IconGridDots = wrap(TbGridDots);
export const IconPlug = wrap(TbPlug);
export const IconActivity = wrap(TbActivity);
export const IconSparkles = wrap(TbSparkles);
export const IconBox = wrap(TbBox);
export const IconPuzzle = wrap(TbPuzzle);
